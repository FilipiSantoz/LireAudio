use crate::metrics::{self, AudioMetrics};
use pipewire::{self as pw, properties::properties, spa};
use ringbuf::{HeapProd, traits::Observer, traits::Producer};
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
        sink_name: Option<String>,
        producer: HeapProd<f32>,
        metrics: Arc<AudioMetrics>,
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

                let mut props = properties! {
                    *pw::keys::MEDIA_TYPE => "Audio",
                    *pw::keys::MEDIA_CATEGORY => "Capture",
                    *pw::keys::MEDIA_ROLE => "Music",
                    *pw::keys::STREAM_CAPTURE_SINK => "true",
                    "node.latency" => "512/48000",
                };

                if let Some(name) = &sink_name {
                    props.insert(*pw::keys::TARGET_OBJECT, name.clone());
                }

                let stream = match pw::stream::StreamBox::new(&core, "luna-capture", props) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_sender.send(Err(format!("Fail to create Stream: {error}")));
                        return;
                    }
                };

                let _stream_listener = match stream
                    .add_local_listener_with_user_data((producer, Arc::clone(&metrics)))
                    .process(|stream, prod| {
                        let (prod, stats) = prod;
                        let Some(mut buffer) = stream.dequeue_buffer() else {
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

                            if prod.vacant_len() >= 2 {
                                if prod.push_slice(&[left, right]) == 2 {
                                    enqueued_frames += 1;
                                } else {
                                    dropped_frames += 1;
                                }
                            } else {
                                dropped_frames += 1;
                            }
                        }
                        stats.capture_callbacks.fetch_add(1, Ordering::Relaxed);
                        stats
                            .capture_frames
                            .fetch_add(frame_count as u64, Ordering::Relaxed);

                        stats
                            .enqueued_frames
                            .fetch_add(enqueued_frames, Ordering::Relaxed);

                        stats
                            .dropped_frames
                            .fetch_add(dropped_frames, Ordering::Relaxed);

                        stats
                            .last_callback_frames
                            .store(frame_count, Ordering::Relaxed);

                        metrics::update_max_usize(&stats.max_callback_frames, frame_count);
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
                let mut params = [pod];

                if let Err(error) = stream.connect(
                    Direction::Input,
                    None,
                    pw::stream::StreamFlags::AUTOCONNECT
                        | pw::stream::StreamFlags::MAP_BUFFERS
                        | pw::stream::StreamFlags::RT_PROCESS,
                    &mut params,
                ) {
                    let _ = ready_sender.send(Err(format!("Fail to connect Stream: {error}")));
                    return;
                }

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
