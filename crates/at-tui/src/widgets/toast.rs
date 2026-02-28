use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    fn color(&self) -> Color {
        match self {
            ToastLevel::Info => Color::Cyan,
            ToastLevel::Success => Color::Green,
            ToastLevel::Warning => Color::Yellow,
            ToastLevel::Error => Color::Red,
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            ToastLevel::Info => "i",
            ToastLevel::Success => "*",
            ToastLevel::Warning => "!",
            ToastLevel::Error => "x",
        }
    }

    /// Returns the default auto-dismiss duration for this toast level.
    /// Error toasts are persistent (24-hour duration, effectively no auto-dismiss).
    pub fn default_duration(&self) -> Duration {
        match self {
            ToastLevel::Info => Duration::from_secs(5),
            ToastLevel::Success => Duration::from_secs(5),
            ToastLevel::Warning => Duration::from_secs(8),
            ToastLevel::Error => Duration::from_secs(86400), // 24 hours — effectively persistent
        }
    }
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created: Instant,
    pub duration: Duration,
}

impl Toast {
    #[allow(dead_code)]
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        let duration = level.default_duration();
        Self {
            message: message.into(),
            level,
            created: Instant::now(),
            duration,
        }
    }

    #[allow(dead_code)]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn expired(&self) -> bool {
        self.created.elapsed() >= self.duration
    }

    /// Returns the fraction of time remaining, from 1.0 (just created) to 0.0 (expired).
    fn remaining_fraction(&self) -> f64 {
        let elapsed = self.created.elapsed().as_secs_f64();
        let total = self.duration.as_secs_f64();
        if total <= 0.0 {
            return 0.0;
        }
        (1.0 - (elapsed / total)).max(0.0)
    }
}

#[allow(dead_code)]
const MAX_TOASTS: usize = 5;
const TOAST_WIDTH: u16 = 40;
const TOAST_HEIGHT: u16 = 3;

/// Manages a stack of toast notifications (max 5).
pub struct ToastManager {
    toasts: VecDeque<Toast>,
}

impl ToastManager {
    pub fn new() -> Self {
        Self { toasts: VecDeque::new() }
    }

    /// Push a new toast. If the stack exceeds the maximum, the oldest toast is removed.
    #[allow(dead_code)]
    pub fn push(&mut self, toast: Toast) {
        self.toasts.push_back(toast);
        if self.toasts.len() > MAX_TOASTS {
            self.toasts.pop_front();
        }
    }

    /// Remove all expired toasts.
    pub fn tick(&mut self) {
        self.toasts.retain(|t| !t.expired());
    }

    /// Returns the number of active (non-expired) toasts.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    /// Returns true if there are no active toasts.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    /// Render the toast stack in the bottom-right corner of `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if self.toasts.is_empty() {
            return;
        }

        let width = TOAST_WIDTH.min(area.width);

        for (i, toast) in self.toasts.iter().rev().enumerate() {
            let index = i as u16;
            let y_offset = (index + 1) * TOAST_HEIGHT;
            if y_offset > area.height {
                break;
            }

            let x = area.x + area.width.saturating_sub(width);
            let y = area.y + area.height.saturating_sub(y_offset);
            let toast_rect = Rect::new(x, y, width, TOAST_HEIGHT);

            // Clear the background behind the toast.
            frame.render_widget(Clear, toast_rect);

            let color = toast.level.color();
            let icon = toast.level.icon();

            // Build the progress bar for remaining time.
            let frac = toast.remaining_fraction();
            let bar_width = (width as usize).saturating_sub(4); // account for border chars
            let filled = ((bar_width as f64) * frac).round() as usize;
            let empty = bar_width.saturating_sub(filled);
            let progress_bar = format!("{}{}", "━".repeat(filled), " ".repeat(empty));

            let lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!(" [{}] ", icon),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&toast.message),
                ]),
                Line::from(Span::styled(
                    format!("  {}", progress_bar),
                    Style::default().fg(color),
                )),
            ];

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .style(Style::default().bg(Color::Black));

            let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

            frame.render_widget(paragraph, toast_rect);
        }
    }
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_expiry() {
        let toast = Toast::new("hello", ToastLevel::Info).with_duration(Duration::from_millis(0));
        assert!(toast.expired());
    }

    #[test]
    fn toast_not_expired() {
        let toast = Toast::new("hello", ToastLevel::Success);
        assert!(!toast.expired());
    }

    #[test]
    fn manager_caps_at_max() {
        let mut mgr = ToastManager::new();
        for i in 0..7 {
            mgr.push(Toast::new(format!("msg {}", i), ToastLevel::Info));
        }
        assert_eq!(mgr.len(), MAX_TOASTS);
    }

    #[test]
    fn manager_tick_removes_expired() {
        let mut mgr = ToastManager::new();
        mgr.push(Toast::new("expired", ToastLevel::Error).with_duration(Duration::from_millis(0)));
        mgr.push(Toast::new("alive", ToastLevel::Info));
        mgr.tick();
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn info_default_duration_is_5s() {
        let toast = Toast::new("info", ToastLevel::Info);
        assert_eq!(toast.duration, Duration::from_secs(5));
    }

    #[test]
    fn success_default_duration_is_5s() {
        let toast = Toast::new("success", ToastLevel::Success);
        assert_eq!(toast.duration, Duration::from_secs(5));
    }

    #[test]
    fn warning_default_duration_is_8s() {
        let toast = Toast::new("warning", ToastLevel::Warning);
        assert_eq!(toast.duration, Duration::from_secs(8));
    }

    #[test]
    fn error_does_not_auto_expire_quickly() {
        let toast = Toast::new("error", ToastLevel::Error);
        // Error toasts should have a very long duration (24 hours).
        assert!(toast.duration >= Duration::from_secs(3600));
        // And should not be expired immediately after creation.
        assert!(!toast.expired());
    }

    #[test]
    fn remaining_fraction_starts_near_one() {
        let toast = Toast::new("test", ToastLevel::Info);
        let frac = toast.remaining_fraction();
        // Just created, should be very close to 1.0.
        assert!(frac > 0.99, "expected fraction near 1.0, got {}", frac);
    }

    #[test]
    fn remaining_fraction_zero_when_expired() {
        let toast = Toast::new("test", ToastLevel::Info).with_duration(Duration::from_millis(0));
        let frac = toast.remaining_fraction();
        assert!((frac - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn default_duration_method_returns_correct_values() {
        assert_eq!(ToastLevel::Info.default_duration(), Duration::from_secs(5));
        assert_eq!(ToastLevel::Success.default_duration(), Duration::from_secs(5));
        assert_eq!(ToastLevel::Warning.default_duration(), Duration::from_secs(8));
        assert_eq!(ToastLevel::Error.default_duration(), Duration::from_secs(86400));
    }
}
