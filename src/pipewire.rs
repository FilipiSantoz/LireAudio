use pipewire::{self as pw};
use std::sync::{Once, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

static PIPEWIRE_INIT: Once = Once::new();

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

    pub fn start(&mut self) -> Result<(), String> {
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
                        let _ =
                            ready_sender.send(Err(format!("Fail to create the Context: {error}")));
                        return;
                    }
                };
                let core = match context.connect_rc(None) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_sender
                            .send(Err(format!("Failt to connect to the Core: {error}")));
                        return;
                    }
                };
                let registry = match core.get_registry_rc() {
                    Ok(value) => value,
                    Err(error) => {
                        let _ =
                            ready_sender.send(Err(format!("Fail to obtain the Registry: {error}")));
                        return;
                    }
                };

                let _registry_listener = registry
                    .add_listener_local()
                    .global(|global| {
                        println!(
                            "[Pipewire] object : id: {}, type: {}/{}",
                            global.id, global.type_, global.version
                        );
                    })
                    .global_remove(|id| {
                        println!("[Pipewire] removed object: id={id}");
                    })
                    .register();

                let loop_to_quit = mainloop.clone();
                let _stop_listener = stop_receiver.attach(mainloop.loop_(), move |_| {
                    println!("[Pipewire] Ending Loop...");
                    loop_to_quit.quit();
                });

                let _ = ready_sender.send(Ok(()));

                println!("[Pipewire] Connected, waiting events");
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
