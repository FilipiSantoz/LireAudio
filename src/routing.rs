#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::LinuxRouter;
#[cfg(target_os = "windows")]
pub use windows::WindowsRouter;

pub trait AudioRouter: Send {
    fn setup(&mut self) -> Result<(), RoutingError>;

    fn teardown(&mut self);

    fn input_device_hint(&self) -> Option<String>;
}

#[derive(Debug, Clone)]
pub enum RoutingError {
    CommandFailed(String),
    ParseFailed(String),
    PlatformNotSupported,
    Other(String),
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed(s) => write!(f, "command failed: {s}"),
            Self::ParseFailed(s) => write!(f, "parse failed: {s}"),
            Self::PlatformNotSupported => write!(f, "platform not supported"),
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for RoutingError {}

pub fn create_router() -> Box<dyn AudioRouter> {
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxRouter::new())
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsRouter::new())
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        struct Unsupported;
        impl AudioRouter for Unsupported {
            fn setup(&mut self) -> Result<(), RoutingError> {
                Err(RoutingError::PlatformNotSupported)
            }
            fn teardown(&mut self) {}
            fn input_device_hint(&self) -> Option<String> {
                None
            }
            fn set_volume(&self, _: u32) {}
            fn volume(&self) -> u32 {
                100
            }
        }
        Box::new(Unsupported)
    }
}
