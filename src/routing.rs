#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::LinuxRouter;
#[cfg(target_os = "windows")]
pub use windows::WindowsRouter;

pub trait AudioRouter: Send {
    /// Cria o sink virtual / configura o roteamento.
    /// Retorna erro se não for possível (ex: Pactl não instalado).
    fn setup(&mut self) -> Result<(), RoutingError>;

    /// Destrói o sink virtual e restaura o sistema ao estado anterior.
    fn teardown(&mut self);

    /// Nome do dispositivo de entrada que o analyzer deve capturar.
    /// None se o roteamento não cria um input próprio (ex: Windows loopback).
    fn input_device_hint(&self) -> Option<String>;

    /// Se true, o analyzer deve filtrar o input pelo hint.
    /// Se false, o usuário escolhe manualmente.
    fn should_auto_select_input(&self) -> bool {
        self.input_device_hint().is_some()
    }
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

/// Factory: escolhe a implementação certa em tempo de compilação.
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
