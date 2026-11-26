//! Editorial SVG summary card generator for Mangora's local reader metrics.
//!
//! Renders a self-contained `1200x675` paper-and-ink summary card styled after
//! `docs/index.html` so users can preview or export their 100 % local reading
//! stats as SVG or rasterize them client-side to PNG without external servers.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::backend::database::ExtendedReaderMetrics;
use crate::config::UiLanguage;
use crate::mangora_appearance::EditorialPalette;

pub const STATS_CARD_WIDTH: u32 = 1200;
pub const STATS_CARD_HEIGHT: u32 = 675;

/// Rendering options for the exportable editorial reading stats card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorialStatsCardOptions {
    pub palette: EditorialPalette,
    pub language: UiLanguage,
    pub reader_alias: Option<String>,
    pub include_screentone: bool,
}

impl Default for EditorialStatsCardOptions {
    fn default() -> Self {
        Self {
            palette: EditorialPalette::ClassicPaper,
            language: UiLanguage::Es,
            reader_alias: None,
            include_screentone: true,
        }
    }
}

fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn truncate_label(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut s: String = trimmed.chars().take(keep).collect();
    s.push('…');
    s
}

fn format_minutes_human(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours == 0 { format!("{mins} min") } else { format!("{hours}h {mins:02}m") }
}

/// Renders a self-contained editorial `1200x675` SVG summary card for `metrics`.
pub fn render_editorial_stats_card_svg(metrics: &ExtendedReaderMetrics, options: &EditorialStatsCardOptions) -> String {
    let spec = options.palette.spec();
    let is_es = matches!(options.language, UiLanguage::Es);

    let bar_title = if is_es { "&gt;_ mangora / resumen del lector" } else { "&gt;_ mangora / reader summary" };
    let eyebrow_text = if is_es { "ESTADÍSTICAS 100 % LOCALES" } else { "100 % LOCAL READING STATS" };
    let palette_name = if is_es { &spec.name_es } else { &spec.name_en };
    let date_badge = metrics.generated_at.split('T').next().unwrap_or("local");
    let alias_clean = options
        .reader_alias
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_label(s, 22));

    let (headline_main, headline_accent) = match (&alias_clean, is_es) {
        (Some(alias), true) => (format!("Bitácora de {},", escape_xml(alias)), "en tinta.".to_string()),
        (Some(alias), false) => (format!("{}'s log,", escape_xml(alias)), "in ink.".to_string()),
        (None, true) => ("Tu lectura,".to_string(), "en tinta.".to_string()),
        (None, false) => ("Your manga,".to_string(), "in ink.".to_string()),
    };

    let kpi_1_label = if is_es { "CAPÍTULOS LEÍDOS" } else { "CHAPTERS READ" };
    let chapters_14d = if metrics.chapters_read_last_14_days > 0 {
        metrics.chapters_read_last_14_days
    } else {
        metrics.daily_activity_last_14_days.iter().map(|d| d.chapters_read).sum()
    };
    let kpi_1_sub = if is_es {
        format!("Hoy {} · Últimos 14d: {}", metrics.chapters_read_today, chapters_14d)
    } else {
        format!("Today {} · Last 14d: {}", metrics.chapters_read_today, chapters_14d)
    };

    let kpi_2_label = if is_es { "TIEMPO DE LECTURA" } else { "READING TIME" };
    let kpi_2_val = format_minutes_human(metrics.estimated_total_reading_minutes);
    let kpi_2_sub = if is_es {
        format!("{:.1} min/cap · {} pág.", metrics.average_minutes_per_chapter, metrics.total_pages_read)
    } else {
        format!("{:.1} min/ch · {} pages", metrics.average_minutes_per_chapter, metrics.total_pages_read)
    };

    let kpi_3_label = if is_es { "RACHA ACTIVA" } else { "ACTIVE STREAK" };
    let kpi_3_val = format!("{} d", metrics.current_streak_days);
    let kpi_3_sub = if is_es {
        format!("Mejor {} d · {} días activos", metrics.longest_streak_days, metrics.active_days_count)
    } else {
        format!("Best {} d · {} active days", metrics.longest_streak_days, metrics.active_days_count)
    };

    let kpi_4_label = if is_es { "BIBLIOTECA" } else { "LIBRARY" };
    let kpi_4_val = format!("{} series", metrics.reading_mangas);
    let kpi_4_sub = format!("{} fav · {} offline", metrics.favorites_count, metrics.downloaded_chapters);

    let genres_title = if is_es { "GÉNEROS MÁS LEÍDOS" } else { "TOP GENRES" };
    let series_title = if is_es { "SERIES DESTACADAS" } else { "TOP SERIES" };
    let activity_title = if is_es { "ACTIVIDAD ÚLTIMOS 14 DÍAS" } else { "LAST 14 DAYS ACTIVITY" };
    let footer_left = if is_es {
        "100 % local en dispositivo · cero telemetría · Mangora"
    } else {
        "100 % on-device local · zero telemetry · Mangora"
    };
    let footer_right = "manga-tui · sqlite compatible";

    let mut svg = String::with_capacity(8192);
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{STATS_CARD_WIDTH}\" height=\"{STATS_CARD_HEIGHT}\" viewBox=\"0 0 {STATS_CARD_WIDTH} {STATS_CARD_HEIGHT}\" role=\"img\" aria-label=\"Mangora Reader Summary\">"
    );
    let _ = writeln!(svg, "  <defs>");
    if options.include_screentone {
        let _ = writeln!(svg, "    <pattern id=\"mangora-screentone\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\">");
        let _ = writeln!(svg, "      <circle cx=\"2\" cy=\"2\" r=\"1.1\" fill=\"{}\" fill-opacity=\"0.35\"/>", spec.red);
        let _ = writeln!(svg, "    </pattern>");
    }
    let _ = writeln!(svg, "  </defs>");

    // Outer paper backdrop
    let _ = writeln!(
        svg,
        "  <rect x=\"0\" y=\"0\" width=\"{STATS_CARD_WIDTH}\" height=\"{STATS_CARD_HEIGHT}\" fill=\"{}\"/>",
        spec.paper
    );

    if options.include_screentone {
        let _ = writeln!(
            svg,
            "  <rect x=\"48\" y=\"24\" width=\"1118\" height=\"608\" fill=\"url(#mangora-screentone)\" stroke=\"{}\" stroke-width=\"1\"/>",
            spec.line
        );
    }

    // Hard offset shadow + primary editorial border
    let _ = writeln!(svg, "  <rect x=\"44\" y=\"44\" width=\"1120\" height=\"595\" fill=\"{}\"/>", spec.ink);
    let _ = writeln!(
        svg,
        "  <rect x=\"36\" y=\"36\" width=\"1120\" height=\"595\" fill=\"{}\" stroke=\"{}\" stroke-width=\"3\"/>",
        spec.paper, spec.ink
    );

    // Top ink window bar (docs/index.html reader-bar)
    let _ = writeln!(svg, "  <rect x=\"36\" y=\"36\" width=\"1120\" height=\"44\" fill=\"{}\"/>", spec.reader_bar_bg);
    let _ = writeln!(
        svg,
        "  <text x=\"60\" y=\"63\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"15\" font-weight=\"700\" fill=\"{}\">{bar_title}</text>",
        spec.reader_bar_fg
    );
    let _ = writeln!(svg, "  <circle cx=\"1096\" cy=\"58\" r=\"4.5\" fill=\"{}\"/>", spec.red);
    let _ = writeln!(
        svg,
        "  <circle cx=\"1114\" cy=\"58\" r=\"4.5\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\"/>",
        spec.reader_bar_fg
    );
    let _ = writeln!(
        svg,
        "  <circle cx=\"1132\" cy=\"58\" r=\"4.5\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\"/>",
        spec.reader_bar_fg
    );

    // Editorial eyebrow + headline + badge
    let _ = writeln!(svg, "  <rect x=\"64\" y=\"110\" width=\"30\" height=\"4\" fill=\"{}\"/>", spec.red);
    let _ = writeln!(
        svg,
        "  <text x=\"104\" y=\"117\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"13\" font-weight=\"700\" letter-spacing=\"1.2\" fill=\"{}\">{}</text>",
        spec.red,
        escape_xml(eyebrow_text)
    );

    let _ = writeln!(
        svg,
        "  <text x=\"64\" y=\"166\" font-family=\"'Arial Black', 'Archivo Black', Arial, sans-serif\" font-size=\"38\" font-weight=\"900\" letter-spacing=\"-1.5\" fill=\"{}\">{headline_main} <tspan fill=\"{}\">{headline_accent}</tspan></text>",
        spec.ink, spec.red
    );

    let badge_text = format!("{} · {}", escape_xml(palette_name), escape_xml(date_badge));
    let _ = writeln!(
        svg,
        "  <rect x=\"846\" y=\"104\" width=\"282\" height=\"32\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1.5\"/>",
        spec.surface, spec.ink
    );
    let _ = writeln!(
        svg,
        "  <text x=\"987\" y=\"125\" text-anchor=\"middle\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"12\" font-weight=\"700\" fill=\"{}\">{badge_text}</text>",
        spec.ink
    );

    // 4 KPI boxes
    let kpis = [
        (kpi_1_label, metrics.total_chapters_read.to_string(), kpi_1_sub),
        (kpi_2_label, kpi_2_val, kpi_2_sub),
        (kpi_3_label, kpi_3_val, kpi_3_sub),
        (kpi_4_label, kpi_4_val, kpi_4_sub),
    ];

    for (idx, (label, val, sub)) in kpis.iter().enumerate() {
        let x = 64 + (idx as u32) * 270;
        let y = 192;
        let _ = writeln!(svg, "  <rect x=\"{}\" y=\"{}\" width=\"254\" height=\"102\" fill=\"{}\"/>", x + 4, y + 4, spec.ink);
        let _ = writeln!(
            svg,
            "  <rect x=\"{x}\" y=\"{y}\" width=\"254\" height=\"102\" fill=\"{}\" stroke=\"{}\" stroke-width=\"2\"/>",
            spec.surface, spec.ink
        );
        let _ = writeln!(
            svg,
            "  <text x=\"{}\" y=\"{}\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"11\" font-weight=\"700\" fill=\"{}\">{}</text>",
            x + 14,
            y + 24,
            spec.muted,
            escape_xml(label)
        );
        let _ = writeln!(
            svg,
            "  <text x=\"{}\" y=\"{}\" font-family=\"'Arial Black', 'Archivo Black', Arial, sans-serif\" font-size=\"30\" font-weight=\"900\" fill=\"{}\">{}</text>",
            x + 14,
            y + 62,
            spec.ink,
            escape_xml(val)
        );
        let _ = writeln!(
            svg,
            "  <text x=\"{}\" y=\"{}\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"11\" fill=\"{}\">{}</text>",
            x + 14,
            y + 86,
            spec.muted,
            escape_xml(sub)
        );
    }

    // Left column: Top Genres with red editorial side stripe
    let _ = writeln!(
        svg,
        "  <rect x=\"64\" y=\"316\" width=\"518\" height=\"242\" fill=\"{}\" stroke=\"{}\" stroke-width=\"2\"/>",
        spec.surface, spec.ink
    );
    let _ = writeln!(svg, "  <rect x=\"64\" y=\"316\" width=\"7\" height=\"242\" fill=\"{}\"/>", spec.red);
    let _ = writeln!(
        svg,
        "  <text x=\"86\" y=\"344\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"13\" font-weight=\"700\" fill=\"{}\">{}</text>",
        spec.ink,
        escape_xml(genres_title)
    );

    if metrics.top_genres.is_empty() {
        let empty_genres = if is_es {
            "Sin géneros registrados todavía en esta biblioteca."
        } else {
            "No manga genres recorded in this local library yet."
        };
        let _ = writeln!(
            svg,
            "  <text x=\"86\" y=\"388\" font-family=\"'Segoe UI', Arial, sans-serif\" font-size=\"14\" fill=\"{}\">{}</text>",
            spec.muted,
            escape_xml(empty_genres)
        );
    } else {
        let max_genre_ch = metrics.top_genres.iter().map(|g| g.chapters_read).max().unwrap_or(1).max(1);
        for (i, genre) in metrics.top_genres.iter().take(5).enumerate() {
            let row_y = 374 + (i as u32) * 36;
            let bar_max_w = 210.0_f32;
            let ratio =
                if genre.chapters_read > 0 { (genre.chapters_read as f32 / max_genre_ch as f32).clamp(0.08, 1.0) } else { 0.08 };
            let bar_w = (bar_max_w * ratio).round() as u32;
            let _ = writeln!(
                svg,
                "  <text x=\"86\" y=\"{row_y}\" font-family=\"'Segoe UI', Arial, sans-serif\" font-size=\"14\" font-weight=\"700\" fill=\"{}\">{}</text>",
                spec.ink,
                escape_xml(&truncate_label(&genre.genre, 18))
            );
            let _ = writeln!(
                svg,
                "  <rect x=\"246\" y=\"{}\" width=\"210\" height=\"14\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>",
                row_y - 12,
                spec.paper,
                spec.ink
            );
            let _ = writeln!(svg, "  <rect x=\"246\" y=\"{}\" width=\"{bar_w}\" height=\"14\" fill=\"{}\"/>", row_y - 12, spec.red);
            let unit = if is_es { "cap" } else { "ch" };
            let _ = writeln!(
                svg,
                "  <text x=\"564\" y=\"{row_y}\" text-anchor=\"end\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"12\" font-weight=\"700\" fill=\"{}\">{} {unit} ({:.0}%)</text>",
                spec.muted, genre.chapters_read, genre.share_percent
            );
        }
    }

    // Right column upper: Top Series
    let _ = writeln!(
        svg,
        "  <rect x=\"602\" y=\"316\" width=\"526\" height=\"134\" fill=\"{}\" stroke=\"{}\" stroke-width=\"2\"/>",
        spec.surface, spec.ink
    );
    let _ = writeln!(
        svg,
        "  <text x=\"620\" y=\"342\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"13\" font-weight=\"700\" fill=\"{}\">{}</text>",
        spec.ink,
        escape_xml(series_title)
    );

    if metrics.top_read_mangas.is_empty() {
        let empty_series =
            if is_es { "Empieza a leer para construir tu ranking local." } else { "Start reading to build your local ranking." };
        let _ = writeln!(
            svg,
            "  <text x=\"620\" y=\"382\" font-family=\"'Segoe UI', Arial, sans-serif\" font-size=\"14\" fill=\"{}\">{}</text>",
            spec.muted,
            escape_xml(empty_series)
        );
    } else {
        for (i, manga) in metrics.top_read_mangas.iter().take(3).enumerate() {
            let row_y = 372 + (i as u32) * 27;
            let unit = if is_es { "cap" } else { "ch" };
            let _ = writeln!(
                svg,
                "  <text x=\"620\" y=\"{row_y}\" font-family=\"'Segoe UI', Arial, sans-serif\" font-size=\"14\" font-weight=\"700\" fill=\"{}\">#{idx} {}</text>",
                spec.ink,
                escape_xml(&truncate_label(&manga.title, 30)),
                idx = i + 1
            );
            let _ = writeln!(
                svg,
                "  <text x=\"1110\" y=\"{row_y}\" text-anchor=\"end\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"12\" font-weight=\"700\" fill=\"{}\">{} {unit} · {}m</text>",
                spec.red, manga.chapters_read, manga.reading_minutes
            );
        }
    }

    // Right column lower: 14-day Activity mini bar chart
    let _ = writeln!(
        svg,
        "  <rect x=\"602\" y=\"464\" width=\"526\" height=\"94\" fill=\"{}\" stroke=\"{}\" stroke-width=\"2\"/>",
        spec.surface, spec.ink
    );
    let _ = writeln!(
        svg,
        "  <text x=\"620\" y=\"486\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"12\" font-weight=\"700\" fill=\"{}\">{}</text>",
        spec.ink,
        escape_xml(activity_title)
    );

    let max_daily = metrics
        .daily_activity_last_14_days
        .iter()
        .map(|d| d.chapters_read)
        .max()
        .unwrap_or(1)
        .max(1);
    for (i, day) in metrics.daily_activity_last_14_days.iter().take(14).enumerate() {
        let bar_x = 622 + (i as u32) * 35;
        let max_h = 44.0_f32;
        let bar_h = if day.chapters_read == 0 {
            4
        } else {
            ((day.chapters_read as f32 / max_daily as f32) * max_h).round().max(8.0) as u32
        };
        let bar_y = 542 - bar_h;
        let fill = if day.chapters_read > 0 { &spec.red } else { &spec.line };
        let _ = writeln!(
            svg,
            "  <rect x=\"{bar_x}\" y=\"{bar_y}\" width=\"22\" height=\"{bar_h}\" fill=\"{fill}\" stroke=\"{}\" stroke-width=\"1\"/>",
            spec.ink
        );
    }

    // Footer strip
    let _ = writeln!(svg, "  <line x1=\"64\" y1=\"576\" x2=\"1128\" y2=\"576\" stroke=\"{}\" stroke-width=\"2\"/>", spec.line);
    let _ = writeln!(
        svg,
        "  <text x=\"64\" y=\"604\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"12\" font-weight=\"700\" fill=\"{}\">{}</text>",
        spec.muted,
        escape_xml(footer_left)
    );
    let _ = writeln!(
        svg,
        "  <text x=\"1128\" y=\"604\" text-anchor=\"end\" font-family=\"'Cascadia Mono', Consolas, monospace\" font-size=\"12\" fill=\"{}\">{footer_right}</text>",
        spec.muted
    );

    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::database::{DailyReadingActivity, GenreReadingStat, TopMangaStat};
    use crate::backend::manga_provider::MangaProviders;

    fn sample_metrics() -> ExtendedReaderMetrics {
        ExtendedReaderMetrics {
            total_chapters_read: 42,
            chapters_read_today: 3,
            chapters_read_last_7_days: 14,
            chapters_read_last_14_days: 28,
            chapters_read_last_30_days: 38,
            total_pages_read: 840,
            pages_read_last_14_days: 560,
            recorded_reading_seconds: 15_120,
            estimated_total_reading_minutes: 252,
            average_minutes_per_chapter: 6.0,
            active_days_count: 11,
            current_streak_days: 4,
            longest_streak_days: 7,
            reading_mangas: 6,
            plan_to_read_mangas: 12,
            favorites_count: 5,
            shelves_count: 3,
            downloaded_chapters: 18,
            top_genres: vec![
                GenreReadingStat {
                    genre: "Seinen".to_string(),
                    mangas_count: 3,
                    chapters_read: 25,
                    reading_minutes: 150,
                    share_percent: 62.5,
                },
                GenreReadingStat {
                    genre: "Fantasy".to_string(),
                    mangas_count: 2,
                    chapters_read: 15,
                    reading_minutes: 90,
                    share_percent: 37.5,
                },
            ],
            top_read_mangas: vec![TopMangaStat {
                manga_id: "m1".to_string(),
                title: "Berserk".to_string(),
                provider: MangaProviders::Mangaoni,
                chapters_read: 25,
                reading_minutes: 150,
            }],
            provider_breakdown: vec![],
            daily_activity_last_14_days: (1..=14)
                .map(|day| DailyReadingActivity {
                    date: format!("2026-11-{day:02}"),
                    chapters_read: day % 4,
                    reading_minutes: (day % 4) * 6,
                    pages_read: (day % 4) * 20,
                })
                .collect(),
            generated_at: "2026-11-24T00:00:00Z".to_string(),
            privacy_local_only: true,
        }
    }

    #[test]
    fn renders_editorial_svg_with_palette_colors_and_xml_escaping() {
        let mut metrics = sample_metrics();
        metrics.top_read_mangas[0].title = "Berserk & <Friends>".to_string();

        let svg_es = render_editorial_stats_card_svg(&metrics, &EditorialStatsCardOptions {
            palette: EditorialPalette::ClassicPaper,
            language: UiLanguage::Es,
            reader_alias: Some("Alvaro <Reader>".to_string()),
            include_screentone: true,
        });

        assert!(svg_es.starts_with("<svg "));
        assert!(svg_es.ends_with("</svg>\n"));
        assert!(svg_es.contains("#f8f7f1"));
        assert!(svg_es.contains("#cb3028"));
        assert!(svg_es.contains("mangora-screentone"));
        assert!(svg_es.contains("Alvaro &lt;Reader&gt;"));
        assert!(svg_es.contains("Berserk &amp; &lt;Friends&gt;"));
        assert!(svg_es.contains("840 pág."));
        assert!(svg_es.contains("100 % local en dispositivo"));

        let svg_night_en = render_editorial_stats_card_svg(&metrics, &EditorialStatsCardOptions {
            palette: EditorialPalette::NightInk,
            language: UiLanguage::En,
            reader_alias: None,
            include_screentone: false,
        });
        assert!(svg_night_en.contains("#141413"));
        assert!(svg_night_en.contains("#ff8a76"));
        assert!(!svg_night_en.contains("mangora-screentone"));
        assert!(svg_night_en.contains("840 pages"));
        assert!(svg_night_en.contains("100 % on-device local"));
    }

    #[test]
    fn renders_empty_metrics_without_panicking() {
        let empty = ExtendedReaderMetrics::default();
        let svg = render_editorial_stats_card_svg(&empty, &EditorialStatsCardOptions::default());
        assert_eq!(svg.contains("Sin géneros registrados"), true);
        assert_eq!(svg.contains("Empieza a leer"), true);
    }
}
