use crate::metrics::{self, PlaybackMetrics, SinkMetrics};
use pipewire::{self as pw, properties::properties, spa};
use ringbuf::HeapCons;
use ringbuf::{HeapProd, traits::Consumer, traits::Observer, traits::Producer};
use spa::param::audio::{AudioFormat, AudioInfoRaw};
use spa::pod::{Pod, Value, serialize::PodSerializer};
use spa::utils::{Direction, SpaTypes};
use std::io::Cursor;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Once, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

static PIPEWIRE_INIT: Once = Once::new();
pub const PIPEWIRE_SAMPLE_RATE: u32 = 48_000;
const STRIDE: usize = 8;

enum EngineCommand {
    Stop,
}

#[derive(Default)]
pub struct PipewireEngine {
    thread_handle: Option<JoinHandle<()>>,
    stop_sender: Option<pw::channel::Sender<EngineCommand>>,
}

impl PipewireEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(
        &mut self,
        producer: HeapProd<f32>,
        consumer: Option<HeapCons<f32>>,
        sink_metrics: Arc<SinkMetrics>,
        playback_metrics: Arc<PlaybackMetrics>,
    ) -> Result<(), String> {
        PIPEWIRE_INIT.call_once(pw::init);

        let (stop_sender, stop_receiver) = pw::channel::channel::<EngineCommand>();
        let (ready_sender, ready_receiver) = mpsc::sync_channel::<Result<(), String>>(1);

        let thread_handle = thread::Builder::new()
            .name("luna-pipewire".into())
            .spawn(move || {
                let mainloop = match pw::main_loop::MainLoopRc::new(None) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ =
                            ready_sender.send(Err(format!("Fail to create Main Loop: {error}")));
                        return;
                    }
                };

                let context = match pw::context::ContextRc::new(&mainloop, None) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_sender.send(Err(format!("Fail to create Context: {error}")));
                        return;
                    }
                };

                let core = match context.connect_rc(None) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_sender.send(Err(format!("Fail to connect to Core: {error}")));
                        return;
                    }
                };

                let mut info = AudioInfoRaw::new();
                info.set_format(AudioFormat::F32LE);
                info.set_rate(PIPEWIRE_SAMPLE_RATE);
                info.set_channels(2);

                let object = spa::pod::Object {
                    type_: SpaTypes::ObjectParamFormat.as_raw(),
                    id: spa::param::ParamType::EnumFormat.as_raw(),
                    properties: info.into(),
                };

                let values: Vec<u8> =
                    match PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
                    {
                        Ok((cursor, _)) => cursor.into_inner(),
                        Err(error) => {
                            let _ = ready_sender
                                .send(Err(format!("Fail to serialize format: {error:?}")));
                            return;
                        }
                    };

                let Some(pod) = Pod::from_bytes(&values) else {
                    let _ = ready_sender.send(Err("Invalid format pod".into()));
                    return;
                };

                // ---
                // Sink Stream
                // ---
                let sink_props = properties! {
                    *pw::keys::MEDIA_TYPE => "Audio",
                    *pw::keys::MEDIA_ROLE => "Music",
                    *pw::keys::NODE_NAME => "luna-sink",
                    *pw::keys::MEDIA_CLASS => "Audio/Sink",
                    *pw::keys::NODE_DESCRIPTION => "Luna Analyzer Sink",
                    "node.latency" => "512/48000",
                    "node.link-group" => "luna-filter",
                    "filter.smart" => "true",
                    "filter.smart.name" => "luna-filter",
                };

                let sink_stream = match pw::stream::StreamBox::new(&core, "luna-sink", sink_props) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_sender.send(Err(format!("Fail to create Stream: {error}")));
                        return;
                    }
                };

                let _sink_listener = match sink_stream
                    .add_local_listener_with_user_data((producer, Arc::clone(&sink_metrics)))
                    .process(|sink_stream, sink_prod| {
                        let (sink_prod, sink_stats) = sink_prod;
                        let Some(mut buffer) = sink_stream.dequeue_buffer() else {
                            return;
                        };
                        let Some(data) = buffer.datas_mut().first_mut() else {
                            return;
                        };

                        let size = data.chunk().size() as usize;
                        let Some(bytes) = data.data() else {
                            return;
                        };
                        let bytes = &bytes[..size.min(bytes.len())];
                        let frame_count = bytes.len() / 8;

                        let mut enqueued_frames = 0_u64;
                        let mut dropped_frames = 0_u64;

                        for frame in bytes.as_chunks::<8>().0 {
                            let left = f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
                            let right =
                                f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);

                            if sink_prod.vacant_len() >= 2 {
                                if sink_prod.push_slice(&[left, right]) == 2 {
                                    enqueued_frames += 1;
                                } else {
                                    dropped_frames += 1;
                                }
                            } else {
                                dropped_frames += 1;
                            }
                        }
                        sink_stats.sink_callbacks.fetch_add(1, Ordering::Relaxed);
                        sink_stats
                            .sink_frames
                            .fetch_add(frame_count as u64, Ordering::Relaxed);

                        sink_stats
                            .enqueued_frames
                            .fetch_add(enqueued_frames, Ordering::Relaxed);

                        sink_stats
                            .dropped_frames
                            .fetch_add(dropped_frames, Ordering::Relaxed);

                        sink_stats
                            .last_callback_frames
                            .store(frame_count, Ordering::Relaxed);

                        metrics::update_max_usize(&sink_stats.max_callback_frames, frame_count);
                    })
                    .register()
                {
                    Ok(listener) => listener,
                    Err(error) => {
                        let _ =
                            ready_sender.send(Err(format!("Fail to register listener: {error}")));
                        return;
                    }
                };

                let mut sink_params = [pod];

                if let Err(error) = sink_stream.connect(
                    Direction::Input,
                    None,
                    pw::stream::StreamFlags::AUTOCONNECT
                        | pw::stream::StreamFlags::MAP_BUFFERS
                        | pw::stream::StreamFlags::RT_PROCESS,
                    &mut sink_params,
                ) {
                    let _ = ready_sender.send(Err(format!("Fail to connect Stream: {error}")));
                    return;
                }

                // ---
                // Playback Stream
                // ---

                let _playback_stream = if let Some(playback_consumer) = consumer {
                    let playback_props = properties! {
                        *pw::keys::MEDIA_TYPE => "Audio",
                        *pw::keys::MEDIA_CATEGORY => "Playback",
                        *pw::keys::MEDIA_ROLE => "Music",
                        *pw::keys::NODE_NAME => "luna-playback",
                        "node.latency" => "512/48000",
                        "node.link-group" => "luna-filter",
                        "node.passive" => "true",
                    };

                    let playback_stream =
                        match pw::stream::StreamBox::new(&core, "Luna-playback", playback_props) {
                            Ok(val) => val,
                            Err(e) => {
                                let _ = ready_sender
                                    .send(Err(format!("Fail to create Playback Node: {e}")));
                                return;
                            }
                        };
                    let playback_listener = match playback_stream
                        .add_local_listener_with_user_data((
                            playback_consumer,
                            Arc::clone(&playback_metrics),
                        ))
                        .process(|stream, state| {
                            let (cons, metrics) = state;
                            metrics.playback_callbacks.fetch_add(1, Ordering::Relaxed);
                            let Some(mut buffer) = stream.dequeue_buffer() else {
                                metrics.no_buffer_callbacks.fetch_add(1, Ordering::Relaxed);
                                return;
                            };
                            let requested_frames = buffer.requested() as usize;
                            let Some(data) = buffer.datas_mut().first_mut() else {
                                return;
                            };
                            let Some(bytes) = data.data() else {
                                metrics.no_buffer_callbacks.fetch_add(1, Ordering::Relaxed);
                                return;
                            };

                            let max_frames = bytes.len() / STRIDE;

                            let frame_count = if requested_frames > 0 {
                                requested_frames.min(max_frames)
                            } else {
                                metrics.no_request_callbacks.fetch_add(1, Ordering::Relaxed);
                                max_frames
                            };
                            if requested_frames > 0 {
                                metrics
                                    .requested_frames
                                    .fetch_add(requested_frames as u64, Ordering::Relaxed);
                            }

                            let output_bytes = &mut bytes[..frame_count * STRIDE];

                            let mut pair = [0.0f32; 2];
                            let mut underrun_frames = 0_u64;

                            for frame in output_bytes.as_chunks_mut::<STRIDE>().0 {
                                if cons.occupied_len() >= 2 && cons.pop_slice(&mut pair) == 2 {
                                    frame[0..4].copy_from_slice(&pair[0].to_le_bytes());
                                    frame[4..8].copy_from_slice(&pair[1].to_le_bytes());
                                } else {
                                    frame.fill(0);
                                    underrun_frames += 1;
                                }
                            }

                            let chunk = data.chunk_mut();
                            *chunk.offset_mut() = 0;
                            *chunk.stride_mut() = STRIDE as i32;
                            *chunk.size_mut() = (frame_count * STRIDE) as u32;

                            metrics
                                .written_frames
                                .fetch_add(frame_count as u64, Ordering::Relaxed);
                            metrics
                                .underrun_frames
                                .fetch_add(underrun_frames, Ordering::Relaxed);
                        })
                        .register()
                    {
                        Ok(l) => l,
                        Err(e) => {
                            let _ = ready_sender
                                .send(Err(format!("Fail to register playback listener: {e}")));
                            return;
                        }
                    };

                    let Some(pod_out) = Pod::from_bytes(&values) else {
                        return;
                    };
                    let mut playback_params = [pod_out];

                    if let Err(e) = playback_stream.connect(
                        Direction::Output,
                        None,
                        pw::stream::StreamFlags::AUTOCONNECT
                            | pw::stream::StreamFlags::MAP_BUFFERS
                            | pw::stream::StreamFlags::RT_PROCESS,
                        &mut playback_params,
                    ) {
                        let _ =
                            ready_sender.send(Err(format!("Fail to connect Playback Stream: {e}")));
                        return;
                    }

                    Some((playback_stream, playback_listener))
                } else {
                    None
                };

                let loop_to_quit = mainloop.clone();
                let _stop_listener = stop_receiver.attach(mainloop.loop_(), move |_| {
                    println!("[Pipewire] Ending Loop...");
                    loop_to_quit.quit();
                });

                let _ = ready_sender.send(Ok(()));
                println!("[Pipewire] Stream connected, capturing");

                mainloop.run();
                println!("[Pipewire] Ended loop");
            })
            .map_err(|error| format!("Fail to create Pipewire Thread: {error}"))?;

        match ready_receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                self.stop_sender = Some(stop_sender);
                self.thread_handle = Some(thread_handle);
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = thread_handle.join();
                Err(error)
            }
            Err(error) => {
                let _ = stop_sender.send(EngineCommand::Stop);
                let _ = thread_handle.join();
                Err(format!("Can't confirm Pipewire Init: {error}"))
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(EngineCommand::Stop);
        }

        if let Some(handle) = self.thread_handle.take()
            && handle.join().is_err()
        {
            eprintln!("[Pipewire] Thread Panicked");
        }
    }
}

impl Drop for PipewireEngine {
    fn drop(&mut self) {
        self.stop();
    }
}
