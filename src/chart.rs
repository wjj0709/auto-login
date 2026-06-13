//! 消耗图表:把 `/api/data/self` 的数组聚合为「按日消耗」与「模型 Top 排行」,
//! 再用 GPUI div 自绘柱状图与排行表。聚合为纯逻辑,带单测。

use gpui::*;

use crate::theme::{Glass, GlassExt};

/// new-api 配额单位换算为美元的系数(与 Python normalize_quota 对齐)。
const QUOTA_PER_USD: f64 = 500000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct DayBar {
    pub day: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelStat {
    pub model: String,
    pub count: i64,
    pub tokens: i64,
    pub amount: f64,
}

fn obj_num(o: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> f64 {
    for k in keys {
        if let Some(n) = o.get(*k).and_then(|v| v.as_f64()) {
            return n;
        }
    }
    0.0
}

fn obj_str(o: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = o.get(*k).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

/// 聚合按日消耗(美元)与模型排行(按消耗降序)。容忍空数组 / 非法 JSON。
pub fn aggregate_chart(chart_json: &str) -> (Vec<DayBar>, Vec<ModelStat>) {
    let mut days: std::collections::BTreeMap<String, f64> = Default::default();
    let mut models: std::collections::HashMap<String, ModelStat> = Default::default();

    if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(chart_json) {
        for item in arr {
            let Some(o) = item.as_object() else { continue };
            let usd = obj_num(o, &["Quota", "quota"]) / QUOTA_PER_USD;

            if let Some(day) = obj_str(o, &["Day", "day", "date"]) {
                if !day.is_empty() {
                    *days.entry(day).or_insert(0.0) += usd;
                }
            }

            let model = obj_str(o, &["ModelName", "model_name", "model"]).unwrap_or_else(|| "未知".into());
            let count = obj_num(o, &["Count", "count", "RequestCount"]) as i64;
            let tokens = obj_num(o, &["TokenUsed", "token_used", "tokens", "PromptTokens"]) as i64;
            let e = models.entry(model.clone()).or_insert(ModelStat {
                model,
                count: 0,
                tokens: 0,
                amount: 0.0,
            });
            e.count += count;
            e.tokens += tokens;
            e.amount += usd;
        }
    }

    let day_bars = days.into_iter().map(|(day, amount)| DayBar { day, amount }).collect();
    let mut model_stats: Vec<ModelStat> = models.into_values().collect();
    model_stats.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
    (day_bars, model_stats)
}

/// 自绘柱状图 + 模型排行。`chart_json` 为缓存的原始数组文本。
pub fn render_chart(chart_json: &str) -> AnyElement {
    let (days, models) = aggregate_chart(chart_json);
    if days.is_empty() && models.is_empty() {
        return div()
            .py_8()
            .text_sm()
            .text_color(Glass::text_muted())
            .child("无消耗数据")
            .into_any_element();
    }

    let max = days.iter().map(|d| d.amount).fold(0.0_f64, f64::max).max(0.000_001);
    let bars: Vec<AnyElement> = days
        .iter()
        .map(|d| {
            let h = ((d.amount / max) * 120.0).max(2.0) as f32;
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_end()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(Glass::text_secondary())
                        .child(format!("${:.2}", d.amount)),
                )
                .child(div().w(px(26.0)).h(px(h)).rounded(px(4.0)).bg(Glass::primary()))
                .child(div().text_xs().text_color(Glass::text_muted()).child(d.day.clone()))
                .into_any_element()
        })
        .collect();

    let rows: Vec<AnyElement> = models
        .iter()
        .take(8)
        .map(|m| {
            div()
                .flex()
                .items_center()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(Glass::border())
                .child(div().flex_1().text_sm().text_color(Glass::text()).child(m.model.clone()))
                .child(
                    div()
                        .w(px(80.0))
                        .text_right()
                        .text_sm()
                        .text_color(Glass::text_secondary())
                        .child(format!("{} 次", m.count)),
                )
                .child(
                    div()
                        .w(px(110.0))
                        .text_right()
                        .text_sm()
                        .text_color(Glass::text_secondary())
                        .child(format!("{} tok", m.tokens)),
                )
                .child(
                    div()
                        .w(px(90.0))
                        .text_right()
                        .text_sm()
                        .text_color(Glass::text())
                        .child(format!("${:.2}", m.amount)),
                )
                .into_any_element()
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div()
                .glass_card()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(Glass::text()).child("近 7 天消耗"))
                .child(div().flex().items_end().justify_between().gap_2().h(px(170.0)).children(bars)),
        )
        .child(
            div()
                .glass_card()
                .overflow_hidden()
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(Glass::text())
                        .child("模型消耗排行"),
                )
                .children(rows),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    // 仅引入被测纯函数,避免 `use super::*` 把 `gpui::*` 拉进测试导致
    // assert_eq! 在 gpui 庞大 trait 表上递归展开触顶。
    use super::aggregate_chart;

    #[test]
    fn aggregate_groups_by_day_and_model() {
        let json = r#"[
            {"Day":"2026-06-12","ModelName":"gpt-4","Quota":500000,"Count":2,"TokenUsed":100},
            {"Day":"2026-06-13","ModelName":"gpt-4","Quota":1000000,"Count":3,"TokenUsed":200},
            {"Day":"2026-06-13","ModelName":"claude","Quota":250000,"Count":1,"TokenUsed":50}
        ]"#;
        let (days, models) = aggregate_chart(json);
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].day, "2026-06-12");
        assert!((days[0].amount - 1.0).abs() < 1e-9);
        assert!((days[1].amount - 2.5).abs() < 1e-9);
        // gpt-4 排第一(消耗 3.0 > claude 0.5)
        assert_eq!(models[0].model, "gpt-4");
        assert_eq!(models[0].count, 5);
        assert_eq!(models[0].tokens, 300);
        assert!((models[0].amount - 3.0).abs() < 1e-9);
        assert_eq!(models[1].model, "claude");
    }

    #[test]
    fn aggregate_tolerates_empty_and_garbage() {
        assert_eq!(aggregate_chart("[]"), (vec![], vec![]));
        assert_eq!(aggregate_chart("not json"), (vec![], vec![]));
        assert_eq!(aggregate_chart("{}"), (vec![], vec![]));
    }
}
