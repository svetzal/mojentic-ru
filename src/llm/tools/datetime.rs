//! Internal local-clock seam shared by date tools.

use crate::error::{MojenticError, Result};
use jiff::ZonedDateTime;

tokio::task_local! {
    static LOCAL_NOW: ZonedDateTime;
}

pub(super) fn now() -> ZonedDateTime {
    LOCAL_NOW.try_with(Clone::clone).unwrap_or_else(|_| ZonedDateTime::now())
}

// Chrono's %+ is RFC 3339 with automatic 0/3/6/9 fractional digits.
// Jiff's strftime intentionally does not implement this directive.
// Chrono also uses POSIX locale formats and numeric local timezone names.
pub(super) fn format(now: &ZonedDateTime, pattern: &str) -> Result<String> {
    let mut expanded = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    let nanos = now.timestamp().subsec_nanosecond();
    let fraction = if nanos == 0 {
        ""
    } else if nanos % 1_000_000 == 0 {
        "%.3f"
    } else if nanos % 1_000 == 0 {
        "%.6f"
    } else {
        "%.9f"
    };
    while let Some(ch) = chars.next() {
        expanded.push(ch);
        if ch != '%' {
            continue;
        }
        match chars.next() {
            Some('+') => {
                expanded.pop();
                expanded.push_str("%Y-%m-%dT%H:%M:%S");
                expanded.push_str(fraction);
                expanded.push_str("%:z");
            }
            Some('f') => expanded.push_str("9f"),
            Some('.') if chars.peek() == Some(&'f') => {
                chars.next();
                expanded.pop();
                expanded.push_str(fraction);
            }
            Some('Z') => {
                expanded.pop();
                expanded.push_str("%:z");
            }
            Some(next) => expanded.push(next),
            None => {}
        }
    }
    let config = jiff::fmt::strtime::Config::new().custom(jiff::fmt::strtime::PosixCustom::new());
    jiff::fmt::strtime::BrokenDownTime::from(now)
        .to_string_with_config(&config, &expanded)
        .map_err(|error| MojenticError::ToolError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::tools::{
        current_datetime_tool::CurrentDatetimeTool, simple_date_tool::SimpleDateTool, LlmTool,
        ToolRunCtx,
    };
    use crate::tracer::{LlmCallTracerEvent, TracerEvent};
    use serde_json::{json, Value};
    use std::collections::HashMap;

    #[tokio::test]
    async fn chrono_era_v1_payloads_remain_compatible() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/datetime/chrono-era-v1.json"
        ))
        .expect("chrono-generated fixture");
        assert_eq!(fixture["contract"], "datetime-compat-v1");
        let instant: jiff::Timestamp = fixture["rfc3339"].as_str().unwrap().parse().unwrap();
        let fixed = instant.to_zoned_datetime(jiff::tz::TimeZone::fixed(
            jiff::tz::Offset::from_seconds(19_800).unwrap(),
        ));
        LOCAL_NOW
            .scope(fixed, async {
                let ctx = ToolRunCtx::default();
                let tool: &dyn LlmTool = &CurrentDatetimeTool;
                assert_eq!(
                    tool.run(&HashMap::new(), &ctx).await.unwrap(),
                    fixture["current_default"]
                );
                let args = HashMap::from([("format_string".into(), json!("%+"))]);
                let output = tool.run(&args, &ctx).await.unwrap();
                assert_eq!(output, fixture["current_rfc3339"]);
                assert_eq!(
                    output["current_datetime"]
                        .as_str()
                        .unwrap()
                        .parse::<jiff::Timestamp>()
                        .unwrap(),
                    instant
                );
                let args = HashMap::from([("format_string".into(), json!("%%+ %Y"))]);
                assert_eq!(tool.run(&args, &ctx).await.unwrap()["current_datetime"], "%+ 2024");
                let args = HashMap::from([("format_string".into(), json!("%!"))]);
                assert!(tool.run(&args, &ctx).await.is_err());
                let tool: &dyn LlmTool = &SimpleDateTool;
                for expected in fixture["dates"].as_array().unwrap() {
                    let args = HashMap::from([(
                        "relative_date".into(),
                        expected["relative_date"].clone(),
                    )]);
                    assert_eq!(tool.run(&args, &ctx).await.unwrap(), *expected);
                }
                let args = HashMap::from([(
                    "relative_date".into(),
                    json!("9223372036854775807 days from now"),
                )]);
                assert!(tool.run(&args, &ctx).await.is_err());
                let args = HashMap::from([(
                    "relative_date".into(),
                    json!("-9223372036854775808 days ago"),
                )]);
                assert!(tool.run(&args, &ctx).await.is_err());
            })
            .await;
        for case in fixture["formats"].as_array().unwrap() {
            let time = jiff::ZonedDateTime::strptime(
                "%Y-%m-%dT%H:%M:%S%.f%:z",
                case["rfc3339"].as_str().unwrap(),
            )
            .unwrap();
            LOCAL_NOW
                .scope(time, async {
                    let args =
                        HashMap::from([("format_string".into(), case["format_string"].clone())]);
                    let output =
                        CurrentDatetimeTool.run(&args, &ToolRunCtx::default()).await.unwrap();
                    assert_eq!(output["current_datetime"], case["current_datetime"], "{case}");
                    assert_eq!(output["timezone"], case["timezone"], "{case}");
                })
                .await;
        }
        let event: LlmCallTracerEvent =
            serde_json::from_value(fixture["tracer_event"].clone()).unwrap();
        assert_eq!(event.timestamp(), fixture["tracer_event"]["timestamp"].as_f64().unwrap());
        assert_eq!(serde_json::to_value(&event).unwrap(), fixture["tracer_event"]);
        assert!(event.printable_summary().contains("fixture-model"));
    }
}
