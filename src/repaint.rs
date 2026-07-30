//! Канал событий, который пробуждает `egui` после изменения фонового состояния.

use egui::Context;
use std::sync::mpsc::{self, Receiver, SendError, Sender};

/// Возвращает настройки `eframe`, устойчивые к мерцанию на мониторах с VRR
/// и высокой частотой обновления.
pub fn stable_native_options() -> eframe::NativeOptions {
    let surface = eframe::SurfaceConfig {
        present_mode: eframe::wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: Some(2),
    };

    let mut options = eframe::NativeOptions {
        dithering: false,
        wgpu_options: eframe::WgpuConfiguration::default().with_surface_config(surface),
        ..Default::default()
    };

    #[cfg(target_os = "windows")]
    {
        options.renderer = eframe::Renderer::Glow;
        options.glow_options.vsync = true;
    }

    options
}

/// Отправитель, запрашивающий новый кадр только после успешной доставки события.
pub struct RepaintSender<T> {
    sender: Sender<T>,
    context: Context,
}

impl<T> Clone for RepaintSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            context: self.context.clone(),
        }
    }
}

impl<T> RepaintSender<T> {
    /// Отправляет событие и будит event loop для отрисовки нового состояния.
    ///
    /// # Errors
    ///
    /// Возвращает [`SendError`] с исходным событием, если получатель уже удалён.
    pub fn send(&self, event: T) -> Result<(), SendError<T>> {
        self.sender.send(event)?;
        self.context.request_repaint();
        Ok(())
    }
}

/// Создает канал для передачи изменений из фоновых потоков в `egui`.
///
/// В отличие от периодического `request_repaint_after`, канал не создает
/// промежуточные кадры: перерисовка происходит только после нового события.
pub fn channel<T>(context: &Context) -> (RepaintSender<T>, Receiver<T>) {
    let (sender, receiver) = mpsc::channel();
    (
        RepaintSender {
            sender,
            context: context.clone(),
        },
        receiver,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_options_use_buffered_vsync_without_dithering() {
        let options = stable_native_options();

        assert!(!options.dithering);
        #[cfg(target_os = "windows")]
        {
            assert_eq!(options.renderer, eframe::Renderer::Glow);
            assert!(options.glow_options.vsync);
        }
        assert_eq!(
            options.wgpu_options.surface.present_mode,
            eframe::wgpu::PresentMode::Fifo
        );
        assert_eq!(
            options.wgpu_options.surface.desired_maximum_frame_latency,
            Some(2)
        );
    }

    #[test]
    fn channel_delivers_events_from_cloned_senders() {
        let (sender, receiver) = channel(&Context::default());
        let cloned_sender = sender.clone();

        sender.send("first").unwrap();
        cloned_sender.send("second").unwrap();

        assert_eq!(receiver.try_iter().collect::<Vec<_>>(), ["first", "second"]);
    }
}
