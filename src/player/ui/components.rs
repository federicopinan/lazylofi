//! Widgets for the analog radio face and its playback controls.

use std::{sync::Arc, time::Duration};

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use crate::{
    player::{LoadPhase, Player},
    tracks::Info,
};

pub const BACKGROUND: Color = Color::Rgb(12, 17, 14);
pub const GREEN: Color = Color::Rgb(157, 209, 146);
pub const AMBER: Color = Color::Rgb(244, 188, 100);
pub const MUTED: Color = Color::Rgb(99, 130, 104);

fn style(color: Color) -> Style {
    Style::default().fg(color).bg(BACKGROUND)
}

fn clipped(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let size = grapheme.width();
        if used + size > width {
            break;
        }
        result.push_str(grapheme);
        used += size;
    }
    result
}

/// Format playback time as minutes and seconds.
pub fn format_duration(duration: &Duration) -> String {
    format!(
        "{:02}:{:02}",
        duration.as_secs() / 60,
        duration.as_secs() % 60
    )
}

/// Show the station frequency prominently above the tuning scale.
pub fn frequency_display(freq: f32) -> Paragraph<'static> {
    Paragraph::new(format!("  {freq:.1} MHz")).style(style(AMBER).add_modifier(Modifier::BOLD))
}

/// Draw an FM scale with a needle that sweeps while scanning.
pub fn dial(freq: f32, anim_tick: u8, width: usize, is_scanning: bool) -> Paragraph<'static> {
    let width = width.saturating_sub(7);
    if width < 3 {
        return Paragraph::new("88 ● 108").style(style(AMBER));
    }
    let needle = if is_scanning {
        usize::from(anim_tick) % width
    } else {
        (((freq.clamp(88.0, 108.0) - 88.0) / 20.0) * (width - 1) as f32).round() as usize
    };
    let mut scale = vec!['─'; width];
    scale[needle] = '●';
    let scale: String = scale.into_iter().collect();
    Paragraph::new(vec![
        Line::from(Span::styled(
            format!("   {}│", " ".repeat(needle)),
            style(AMBER),
        )),
        Line::from(vec![
            Span::styled("88 ", style(MUTED)),
            Span::styled(scale, style(AMBER)),
            Span::styled(" 108", style(MUTED)),
        ]),
    ])
}

/// Render the current tuning step, animating only while loading.
pub fn load_status(phase: LoadPhase, anim_tick: u8, width: usize) -> Paragraph<'static> {
    const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let message = match phase {
        LoadPhase::Scanning => format!(
            "{} scanning stations...",
            SPINNER[usize::from(anim_tick) % 10]
        ),
        LoadPhase::Tuning => format!("{} tuning in...", SPINNER[usize::from(anim_tick) % 10]),
        LoadPhase::Locked { freq } => format!("● locked · {freq:.1} MHz"),
    };
    Paragraph::new(clipped(&message, width)).style(style(
        if matches!(phase, LoadPhase::Locked { .. }) {
            GREEN
        } else {
            AMBER
        },
    ))
}

/// Genre label, or the custom track-list indicator.
pub fn genre(player: &Player, width: usize) -> Paragraph<'static> {
    let name = if player.has_custom_tracks {
        "tracks".to_owned()
    } else {
        player
            .list
            .try_read()
            .map(|list| list.name.clone())
            .unwrap_or_default()
    };
    Paragraph::new(clipped(&format!("▓ {name}"), width)).style(style(GREEN))
}

/// Show the current track or a placeholder while no track is available.
pub fn track(current: Option<&Arc<Info>>, width: usize) -> Paragraph<'static> {
    let name = current.map_or("waiting for signal", |info| info.name.as_str());
    Paragraph::new(clipped(&format!("♪ {name}"), width)).style(style(AMBER))
}

/// Render a playback bar and elapsed/total time on separate rows.
pub fn progress_bar(
    player: &Player,
    current: Option<&Arc<Info>>,
    width: usize,
) -> Paragraph<'static> {
    let elapsed = current.map_or(Duration::ZERO, |_| player.sink.get_pos());
    let duration = current
        .and_then(|info| info.duration)
        .unwrap_or(Duration::ZERO);
    let bar_width = width.saturating_sub(3);
    let progress = if duration.is_zero() {
        0.0
    } else {
        (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0)
    };
    let filled = (progress * bar_width as f64).round() as usize;
    Paragraph::new(vec![
        Line::from(vec![
            Span::styled("◀", style(MUTED)),
            Span::styled("━".repeat(filled), style(AMBER)),
            Span::styled("●", style(AMBER)),
            Span::styled("━".repeat(bar_width.saturating_sub(filled)), style(MUTED)),
            Span::styled("▶", style(MUTED)),
        ]),
        Line::from(Span::styled(
            format!(
                "{} / {}",
                format_duration(&elapsed),
                format_duration(&duration)
            ),
            style(GREEN),
        )),
    ])
}

/// Show the temporary volume adjustment in place of the playback bar.
pub fn audio_bar(volume: f32, width: usize) -> Paragraph<'static> {
    let bar_width = width.saturating_sub(12);
    let filled = ((volume.clamp(0.0, 1.0) * bar_width as f32).round() as usize).min(bar_width);
    Paragraph::new(format!(
        "volume {}{} {:>3}%",
        "━".repeat(filled),
        "─".repeat(bar_width - filled),
        (volume * 100.0).round() as u16
    ))
    .style(style(AMBER))
}

/// Render the available keyboard shortcuts, dimming the disabled picker.
pub fn controls(player: &Player, width: usize) -> Paragraph<'static> {
    let entries = [
        ("[s]kip", false),
        ("[p]ause", false),
        ("[g]enre", player.has_custom_tracks),
        ("[q]uit", false),
    ];
    let mut remaining = width;
    let mut spans = Vec::new();
    for (index, (label, disabled)) in entries.iter().enumerate() {
        if index > 0 {
            if remaining < 2 {
                break;
            }
            spans.push(Span::raw("  "));
            remaining -= 2;
        }
        let text = clipped(label, remaining);
        remaining -= text.width();
        spans.push(Span::styled(
            text,
            style(if *disabled { MUTED } else { GREEN }),
        ));
    }
    Paragraph::new(Line::from(spans))
}

/// Build a single row of the animated ocean decoration.
///
/// `y` selects which set of characters to draw: 0 picks the soft
/// wave-crest glyphs; any other value picks the block characters used
/// for the body of the sea. `tick` advances the phase so the wave
/// scrolls horizontally on every frame.
fn wave_row(y: usize, tick: u8, width: usize) -> String {
    let chars: &[char] = if y == 0 {
        &['~', '∿', '∾', '~']
    } else {
        &['░', '▒', '▓', '░']
    };
    let speed = if y == 0 { 0.20 } else { 0.15 };
    let freq = if y == 0 { 0.40 } else { 0.30 };
    let phase_offset = if y == 0 { 0.0 } else { 1.5 };
    (0..width)
        .map(|x| {
            let phase = (x as f32 * freq + f32::from(tick) * speed + phase_offset).sin();
            let idx = ((phase + 1.0) * (chars.len() as f32 / 2.0)) as usize;
            chars[idx.min(chars.len() - 1)]
        })
        .collect()
}

/// Animated ocean-themed decoration drawn at the bottom of the radio
/// face. Two lines (crest + body) scroll left/right using sine
/// functions of `anim_tick` and the column index, so the pattern
/// looks like a continuous wave rather than a static pattern.
pub fn wave(anim_tick: u8, width: usize) -> Paragraph<'static> {
    let crest: String = wave_row(0, anim_tick, width);
    let body: String = wave_row(1, anim_tick, width);
    let line = |row: &String| {
        Line::from(
            row.chars()
                .map(|c| Span::styled(c.to_string(), style(AMBER)))
                .collect::<Vec<_>>(),
        )
    };
    Paragraph::new(vec![line(&crest), line(&body)])
}
