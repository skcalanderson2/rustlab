//! Shared widget styles: subtle, JupyterLab-like chrome.

use iced::widget::button;
use iced::Theme;

/// Flat toolbar/icon button: transparent at rest, soft tint on hover,
/// hairline outline — no heavy filled background.
pub fn toolbar_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(if hovered {
            palette.background.weak.color.into()
        } else {
            iced::Color::TRANSPARENT.into()
        }),
        text_color: palette.background.base.text,
        border: iced::Border {
            color: palette.background.strong.color.scale_alpha(if hovered {
                0.8
            } else {
                0.35
            }),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Active tab: white with an accent underline feel — subtle, not filled blue.
pub fn active_tab(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let _ = status;
    button::Style {
        background: Some(palette.background.base.color.into()),
        text_color: palette.background.base.text,
        border: iced::Border {
            color: palette.primary.base.color.scale_alpha(0.7),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Inactive tab: quiet text, no chrome until hovered.
pub fn inactive_tab(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(if hovered {
            palette.background.weak.color.into()
        } else {
            iced::Color::TRANSPARENT.into()
        }),
        text_color: palette.background.base.text.scale_alpha(0.75),
        border: iced::Border {
            radius: 4.0.into(),
            ..iced::Border::default()
        },
        ..button::Style::default()
    }
}
