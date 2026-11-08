use super::size::{SizeTier, parse_size, round_to_alignment, size_tier, validate_size};

pub const STANDARD_MODEL: &str = "gpt-image-2";
// 本站只有一条生图线路。上游 micu-image-mcp 的目录里还有 gpt-image-2-openai 与
// 2.5/2.6 的 Flare/Sunburst，这些模型名在本站背后都没有渠道：登记进来，≥2K 请求会被
// 自动切过去，客户拿到的是「模型未配置价格」。
//
// 下面几个函数保留上游的形状（别名映射、后缀补试、质量线路判定），在单模型目录下都退化
// 为恒等或「否」。这样调用处与上游一致，日后同步上游、或本站开通新线路时，只需扩充这里。
pub const SUPPORTED_IMAGE_MODELS: [&str; 1] = [STANDARD_MODEL];

pub fn canonical_model(model: &str) -> &str {
    model
}

/// 上游用它在模型改名后补试带 `-openai` 后缀的旧 ID；本站没有别名，恒为 None。
pub fn suffix_fallback(_model: &str) -> Option<&'static str> {
    None
}

const BASE_QUALITY_VALUES: [&str; 4] = ["auto", "high", "low", "medium"];
const GIF_FORMAT: &str = "gif";
// GIF 只有 gpt-image-2.6 能出，本站没有这条线路，因此不开放。
const OUTPUT_FORMAT_VALUES: [&str; 3] = ["jpeg", "png", "webp"];
const STANDARD_PARALLEL_LIMIT: usize = 5;

pub fn is_supported_model(model: &str) -> bool {
    SUPPORTED_IMAGE_MODELS.contains(&canonical_model(model))
}

/// Accepted `quality` values, sorted as the public server_info contract lists them.
pub fn quality_values(_model: &str) -> &'static [&'static str] {
    &BASE_QUALITY_VALUES
}

/// Accepted `output_format` values, sorted as the public server_info contract lists them.
pub fn output_format_values(_model: &str) -> &'static [&'static str] {
    &OUTPUT_FORMAT_VALUES
}

/// Upstream retry scheduling chosen by the route; the HTTP executor only applies it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryOptions {
    pub enabled: bool,
    pub big_size: bool,
}

/// How one supported model runs at one validated size: the effective model,
/// quality choices, retry schedule, parallelism and response-format negotiation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRoute {
    model: String,
    tier: SizeTier,
}

impl ModelRoute {
    /// `size` must already pass `validate_size`, and callers reject unsupported
    /// models first so that model errors keep precedence over size errors.
    pub fn resolve(
        requested: Option<&str>,
        default_model: &str,
        size: &str,
    ) -> (Self, Vec<String>) {
        let requested_model = requested.unwrap_or(default_model);
        let model = canonical_model(requested_model).to_owned();
        let tier = size_tier(size);
        let mut notes = Vec::new();
        if model != requested_model {
            notes.push(format!("模型 ID {requested_model} 已更新为 {model}"));
        }
        // 不按尺寸切模型：只有一条线路。大尺寸照常请求，但要提前说清拿不到精确像素 ——
        // size 只是建议，实测请求 1024x1024 会回 1122x1402。
        if is_large_tier(tier) {
            notes.push(format!(
                "size={size} ({}) 会照常请求，但本站线路不保证精确像素，请核对 saved.actual_size",
                tier_label(tier)
            ));
        }
        (Self { model, tier }, notes)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn retry(&self) -> RetryOptions {
        let big_size = is_large_tier(self.tier);
        RetryOptions {
            enabled: is_quality_model(&self.model) || big_size,
            big_size,
        }
    }

    /// Only standard ≤1K routes fan out; quality and ≥2K routes run one request at a time.
    pub fn parallel_limit(&self) -> usize {
        if matches!(self.tier, SizeTier::Small | SizeTier::OneK) && !is_quality_model(&self.model) {
            STANDARD_PARALLEL_LIMIT
        } else {
            1
        }
    }

    /// 2K/4K generations are limited to one image per call.
    pub fn cap_n(&self, requested: usize) -> (usize, Option<String>) {
        if is_large_tier(self.tier) && requested > 1 {
            let note = format!(
                "{} 强制 N=1，已忽略请求的 n={requested}",
                tier_label(self.tier).to_ascii_uppercase()
            );
            (1, Some(note))
        } else {
            (requested, None)
        }
    }

    pub fn validate_quality(&self, quality: Option<&str>) -> Result<Option<String>, String> {
        validate_choice("quality", quality, quality_values(&self.model))
    }

    pub fn validate_output_format(&self, format: Option<&str>) -> Result<Option<String>, String> {
        validate_choice("output_format", format, output_format_values(&self.model))
    }

    /// gpt-image-2 negotiates response format (url, then b64_json). The Flare and
    /// Sunburst routes of the upstream catalog omit it; this catalog has none of them.
    pub fn response_formats<'a>(
        &self,
        configured: &'a [&'static str],
    ) -> impl Iterator<Item = Option<&'static str>> + 'a {
        let explicit = self.model == STANDARD_MODEL;
        configured
            .iter()
            .take(if explicit { configured.len() } else { 1 })
            .map(move |format| explicit.then_some(*format))
    }
}

fn validate_choice(
    field: &str,
    value: Option<&str>,
    allowed: &[&str],
) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let cleaned = raw.trim().to_ascii_lowercase();
    if cleaned.is_empty() {
        return Ok(None);
    }
    if allowed.contains(&cleaned.as_str()) {
        Ok(Some(cleaned))
    } else {
        Err(format!(
            "{field} 不支持 {}；可选 {}",
            python_string_repr(raw),
            allowed.join(" / ")
        ))
    }
}

pub fn output_format_note(requested: Option<&str>, actual: &str) -> Option<String> {
    let requested = requested?;
    (requested != actual).then(|| {
        let hint = if requested == GIF_FORMAT {
            "；动图需要在 prompt 中明确要求动画"
        } else {
            ""
        };
        format!("⚠ 请求 output_format={requested}，实际返回 {actual}；以 saved[].format 为准{hint}")
    })
}

pub fn infer_size_from_prompt(prompt: &str) -> Option<(String, String)> {
    let normalized = prompt.to_lowercase();
    if let Some((width, height)) = find_explicit_pixels(&normalized) {
        let aligned_width = round_to_alignment(width);
        let aligned_height = round_to_alignment(height);
        let inferred = format!("{aligned_width}x{aligned_height}");
        if validate_size(Some(&inferred), false).1.is_some() {
            return None;
        }
        let reason = if aligned_width != width || aligned_height != height {
            format!(
                "prompt 含像素 {width}x{height}，对齐 16 倍数为 {aligned_width}x{aligned_height}"
            )
        } else {
            format!("prompt 含明确像素 {width}x{height}")
        };
        return Some((inferred, reason));
    }

    let vertical_keywords = [
        "9:16",
        "竖屏",
        "竖版",
        "vertical",
        "portrait",
        "phone wallpaper",
        "tiktok",
        "reels",
        "stories",
        "手机壁纸",
    ];
    let horizontal_keywords = [
        "16:9",
        "横屏",
        "横版",
        "landscape",
        "widescreen",
        "desktop wallpaper",
        "wallpaper",
        "壁纸",
        "banner",
        "封面",
        "cover",
    ];
    let square_keywords = [
        "正方形",
        "square",
        "avatar",
        "头像",
        "icon",
        "logo",
        "profile pic",
        "头像图",
        "图标",
    ];
    let poster_keywords = ["poster", "海报", "2:3", "movie poster"];
    let photo_keywords = ["3:2", "photograph", "照片"];
    let is_vertical = contains_any(&normalized, &vertical_keywords);
    let is_horizontal = contains_any(&normalized, &horizontal_keywords);
    let is_square = contains_any(&normalized, &square_keywords);
    let is_poster = contains_any(&normalized, &poster_keywords);
    let is_photo = contains_any(&normalized, &photo_keywords);

    if contains_word(&normalized, "4k")
        || normalized.contains("uhd")
        || normalized.contains("ultra hd")
        || normalized.contains("ultra-hd")
        || normalized.contains("ultrahd")
        || normalized.contains("超高清")
    {
        return if is_vertical {
            Some(("2160x3840".into(), "prompt 含 4K 关键字 + 竖屏".into()))
        } else {
            Some(("3840x2160".into(), "prompt 含 4K 关键字（默认横屏）".into()))
        };
    }
    if contains_word(&normalized, "2k")
        || normalized.contains("1080p")
        || normalized.contains("full hd")
        || normalized.contains("full-hd")
        || normalized.contains("fullhd")
        || contains_word(&normalized, "fhd")
    {
        return if is_vertical {
            Some((
                "1152x2048".into(),
                "prompt 含 2K/1080p 关键字 + 竖屏".into(),
            ))
        } else {
            Some((
                "2048x1152".into(),
                "prompt 含 2K/1080p 关键字（默认横屏）".into(),
            ))
        };
    }
    if normalized.contains("720p") || contains_word(&normalized, "hd") {
        return if is_vertical {
            Some(("720x1280".into(), "prompt 含 720p 关键字 + 竖屏".into()))
        } else {
            Some(("1280x720".into(), "prompt 含 720p 关键字".into()))
        };
    }
    if is_square {
        return Some(("1024x1024".into(), "prompt 含正方形/logo/头像关键字".into()));
    }
    if is_poster {
        return Some(("1024x1536".into(), "prompt 含海报/2:3 关键字".into()));
    }
    if is_photo {
        return Some(("1536x1024".into(), "prompt 含照片/3:2 关键字".into()));
    }
    if is_vertical {
        return Some(("1024x1536".into(), "prompt 含竖屏关键字（1K 默认）".into()));
    }
    if is_horizontal {
        return Some(("1536x1024".into(), "prompt 含横屏关键字（1K 默认）".into()));
    }
    None
}

/// A returned GIF gets a neutral note because the service picks its size.
/// This site has no exact-pixel route, so no note ever recommends one.
pub fn size_note(
    requested: &str,
    actual: Option<(u32, u32)>,
    requested_format: Option<&str>,
    actual_format: &str,
) -> Option<String> {
    let (requested_width, requested_height) = parse_size(requested)?;
    let (actual_width, actual_height) = actual?;
    if (requested_width, requested_height) == (actual_width, actual_height) {
        return None;
    }
    if actual_format == GIF_FORMAT {
        return Some(format!(
            "GIF 实际 {actual_width}×{actual_height}：尺寸由服务端决定，不保证遵循 size；以 saved[].actual_size 为准。"
        ));
    }
    let requested_megapixels =
        f64::from(requested_width) * f64::from(requested_height) / 1_000_000.0;
    let actual_megapixels = f64::from(actual_width) * f64::from(actual_height) / 1_000_000.0;
    let guidance = if requested_format == Some(GIF_FORMAT) {
        "涉及 GIF 时尺寸和宽高比不保证遵循请求，请以 saved[].actual_size 为准。"
    } else {
        "本站生图不保证精确像素，size 只是建议，请以 saved.actual_size 为准；需要精确构图时把画幅要求写进 prompt。"
    };
    Some(format!(
        "⚠ 实际 {actual_width}×{actual_height} ({actual_megapixels:.2}MP) ≠ 请求 {requested_width}×{requested_height} ({requested_megapixels:.2}MP)；{guidance}"
    ))
}

/// 上游的「质量线路」（gpt-image-2-openai、Sunburst）一律串行、一律重试；本站没有这类线路。
/// ≥2K 的串行与重试按尺寸判定，见 `ModelRoute::retry` 与 `parallel_limit`。
pub fn is_quality_model(_model: &str) -> bool {
    false
}

pub fn is_large_tier(tier: SizeTier) -> bool {
    matches!(tier, SizeTier::TwoK | SizeTier::FourK)
}

fn tier_label(tier: SizeTier) -> &'static str {
    match tier {
        SizeTier::Unknown => "unknown",
        SizeTier::Small => "small",
        SizeTier::OneK => "1k",
        SizeTier::TwoK => "2k",
        SizeTier::FourK => "4k",
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(index, _)| {
        let before = haystack[..index].chars().next_back();
        let after = haystack[index + needle.len()..].chars().next();
        !before.is_some_and(is_word_character) && !after.is_some_and(is_word_character)
    })
}

fn find_explicit_pixels(prompt: &str) -> Option<(u32, u32)> {
    let bytes = prompt.as_bytes();
    for start in 0..bytes.len() {
        if !bytes[start].is_ascii_digit() {
            continue;
        }
        let mut cursor = start;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() && cursor - start < 4 {
            cursor += 1;
        }
        if cursor - start < 3 {
            continue;
        }
        let first_end = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor < bytes.len() && matches!(bytes[cursor], b'x' | b'X') {
            cursor += 1;
        } else if bytes.get(cursor..cursor.saturating_add(2)) == Some("×".as_bytes()) {
            cursor += 2;
        } else {
            continue;
        }
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let second_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() && cursor - second_start < 4 {
            cursor += 1;
        }
        if cursor - second_start < 3 {
            continue;
        }
        let width = prompt[start..first_end].parse().ok()?;
        let height = prompt[second_start..cursor].parse().ok()?;
        return Some((width, height));
    }
    None
}

fn python_string_repr(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(model: &str, size: &str) -> ModelRoute {
        ModelRoute::resolve(Some(model), STANDARD_MODEL, size).0
    }

    #[test]
    fn output_format_is_optional_and_gif_is_not_offered() {
        let route = route(STANDARD_MODEL, "1024x1024");
        assert_eq!(route.validate_output_format(None), Ok(None));
        assert_eq!(route.validate_output_format(Some("  ")), Ok(None));
        assert_eq!(
            route.validate_output_format(Some(" PNG ")),
            Ok(Some("png".into()))
        );
        assert_eq!(
            route.validate_output_format(Some("webp")),
            Ok(Some("webp".into()))
        );
        for rejected in ["gif", "jpg"] {
            let expected = format!("output_format 不支持 '{rejected}'");
            assert!(
                route
                    .validate_output_format(Some(rejected))
                    .is_err_and(|error| error.contains(&expected)),
                "{rejected}"
            );
        }
    }

    #[test]
    fn output_format_note_only_reports_a_mismatch() {
        assert_eq!(output_format_note(None, "png"), None);
        assert_eq!(output_format_note(Some("gif"), "gif"), None);
        assert!(
            output_format_note(Some("gif"), "png")
                .is_some_and(|note| note.contains("png") && note.contains("prompt"))
        );
        assert!(
            output_format_note(Some("webp"), "png").is_some_and(|note| !note.contains("prompt"))
        );
    }

    #[test]
    fn model_contract_is_exactly_one_route() {
        assert_eq!(SUPPORTED_IMAGE_MODELS, [STANDARD_MODEL]);
        assert!(is_supported_model(STANDARD_MODEL));
        // 上游目录里的其他模型在本站背后没有渠道，必须在本地就拒掉。
        for model in [
            " gpt-image-2 ",
            "GPT-IMAGE-2",
            "gpt-image-2-openai",
            "gpt-image-2.5-flare",
            "gpt-image-2.5-sunburst",
            "gpt-image-2.6-flare",
            "gpt-image-2.6-sunburst",
            "grok-imagine-image",
        ] {
            assert!(!is_supported_model(model), "{model}");
        }
        assert_eq!(canonical_model(STANDARD_MODEL), STANDARD_MODEL);
        assert_eq!(suffix_fallback(STANDARD_MODEL), None);
        assert!(!is_quality_model(STANDARD_MODEL));
    }

    #[test]
    fn route_table_covers_every_size_tier() {
        // (size, retry enabled, big size, parallel limit)
        let table = [
            ("512x512", false, false, 5),
            ("1024x1024", false, false, 5),
            ("2048x1152", true, true, 1),
            ("3840x2160", true, true, 1),
        ];
        let configured = ["url", "b64_json"];
        for (size, enabled, big_size, parallel) in table {
            let route = route(STANDARD_MODEL, size);
            assert_eq!(route.model(), STANDARD_MODEL, "{size}: 不按尺寸切模型");
            assert_eq!(route.retry(), RetryOptions { enabled, big_size }, "{size}");
            assert_eq!(route.parallel_limit(), parallel, "{size}");
            assert_eq!(
                route.response_formats(&configured).collect::<Vec<_>>(),
                vec![Some("url"), Some("b64_json")],
                "{size}"
            );
        }
    }

    #[test]
    fn large_tiers_cap_generation_count_with_public_note() {
        assert_eq!(route(STANDARD_MODEL, "1024x1024").cap_n(4), (4, None));
        assert_eq!(
            route(STANDARD_MODEL, "2048x1152").cap_n(4),
            (1, Some("2K 强制 N=1，已忽略请求的 n=4".into()))
        );
        assert_eq!(route(STANDARD_MODEL, "3840x2160").cap_n(1), (1, None));
    }

    #[test]
    fn quality_values_are_the_standard_four() {
        let small = route(STANDARD_MODEL, "1024x1024");
        assert_eq!(
            small.validate_quality(Some(" HIGH ")),
            Ok(Some("high".into()))
        );
        assert_eq!(small.validate_quality(Some("  ")), Ok(None));
        assert_eq!(small.validate_quality(None), Ok(None));
        assert_eq!(
            small.validate_quality(Some("max")),
            Err("quality 不支持 'max'；可选 auto / high / low / medium".into())
        );
        assert!(
            route(STANDARD_MODEL, "2048x1152")
                .validate_quality(Some("xhigh"))
                .is_err()
        );
    }

    #[test]
    fn high_resolution_stays_on_the_single_route_with_public_note() {
        let (route, notes) = ModelRoute::resolve(None, STANDARD_MODEL, "1024x1024");
        assert_eq!((route.model(), notes.len()), (STANDARD_MODEL, 0));
        let (route, notes) = ModelRoute::resolve(Some(STANDARD_MODEL), STANDARD_MODEL, "2048x1152");
        assert_eq!(route.model(), STANDARD_MODEL);
        assert_eq!(
            notes,
            vec![
                "size=2048x1152 (2k) 会照常请求，但本站线路不保证精确像素，请核对 saved.actual_size"
            ]
        );
    }

    #[test]
    fn prompt_inference_preserves_priority_and_python_rounding() {
        assert_eq!(
            infer_size_from_prompt("画一张 1920x1080 的图").map(|item| item.0),
            Some("1920x1088".into())
        );
        assert_eq!(
            infer_size_from_prompt("4K vertical 海报").map(|item| item.0),
            Some("2160x3840".into())
        );
        assert_eq!(
            infer_size_from_prompt("FullHD vertical phone wallpaper").map(|item| item.0),
            Some("1152x2048".into())
        );
        assert_eq!(
            infer_size_from_prompt("a minimalist logo").map(|item| item.0),
            Some("1024x1024".into())
        );
        assert_eq!(infer_size_from_prompt("4K landscape 1024x512 layout"), None);
        assert_eq!(infer_size_from_prompt("a red apple"), None);
    }

    #[test]
    fn size_mismatch_note_is_stable_and_names_no_other_route() {
        assert_eq!(
            size_note("1024x1024", Some((1024, 1024)), None, "png"),
            None
        );
        for requested_format in [None, Some("png"), Some("webp")] {
            let note = size_note("3840x2160", Some((2048, 1152)), requested_format, "png")
                .unwrap_or_default();
            assert!(note.contains("实际 2048×1152"), "{note}");
            assert!(note.contains("≠ 请求 3840×2160"), "{note}");
            assert!(note.contains("saved.actual_size"), "{note}");
            assert!(!note.contains("gpt-image-2-openai"), "{note}");
            assert!(!note.contains("上游"), "{note}");
        }
    }

    #[test]
    fn gif_size_note_never_recommends_another_route() {
        assert_eq!(
            size_note("1024x1024", Some((1024, 1024)), Some("gif"), "gif"),
            None
        );
        for (requested_format, actual_format) in [
            (Some("gif"), "gif"),
            (None, "gif"),
            (Some("png"), "gif"),
            (Some("gif"), "png"),
        ] {
            let note = size_note(
                "1024x1024",
                Some((410, 410)),
                requested_format,
                actual_format,
            )
            .unwrap_or_default();
            assert!(note.contains("实际 410×410"), "{note}");
            assert!(note.contains("GIF"), "{note}");
            assert!(!note.contains("gpt-image-2-openai"), "{note}");
            assert_eq!(note.starts_with('⚠'), actual_format != "gif", "{note}");
        }
    }
}
