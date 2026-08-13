//! Mark-local rows（TanStack：各 mark 直接吃行，不强制 series 包一层）。

use std::rc::Rc;

/// 一行观测。`series` ≈ TanStack `z`；`time` 为 Unix **毫秒**（时间轴）。
#[derive(Clone, Debug, PartialEq)]
pub struct ChartDatum {
    pub label: String,
    pub value: f32,
    pub series: Option<String>,
    pub time: Option<i64>,
}

impl ChartDatum {
    pub fn new(label: impl Into<String>, value: f32) -> Self {
        Self {
            label: label.into(),
            value,
            series: None,
            time: None,
        }
    }

    pub fn at(unix_ms: i64, value: f32) -> Self {
        Self {
            label: String::new(),
            value,
            series: None,
            time: Some(unix_ms),
        }
    }

    pub fn series(mut self, name: impl Into<String>) -> Self {
        self.series = Some(name.into());
        self
    }

    pub fn time(mut self, unix_ms: i64) -> Self {
        self.time = Some(unix_ms);
        self
    }
}

impl From<f32> for ChartDatum {
    fn from(value: f32) -> Self {
        Self::new("", value)
    }
}

impl From<(String, f32)> for ChartDatum {
    fn from((label, value): (String, f32)) -> Self {
        Self::new(label, value)
    }
}

impl From<(&str, f32)> for ChartDatum {
    fn from((label, value): (&str, f32)) -> Self {
        Self::new(label, value)
    }
}

impl From<(String, f32, String)> for ChartDatum {
    fn from((label, value, series): (String, f32, String)) -> Self {
        Self::new(label, value).series(series)
    }
}

impl From<(&str, f32, &str)> for ChartDatum {
    fn from((label, value, series): (&str, f32, &str)) -> Self {
        Self::new(label, value).series(series)
    }
}

impl From<(i64, f32)> for ChartDatum {
    fn from((unix_ms, value): (i64, f32)) -> Self {
        Self::at(unix_ms, value)
    }
}

/// RSX `selected={…}`：可传 `usize` 或 `Option<usize>`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChartSelected(pub Option<usize>);

impl From<usize> for ChartSelected {
    fn from(i: usize) -> Self {
        Self(Some(i))
    }
}

impl From<Option<usize>> for ChartSelected {
    fn from(i: Option<usize>) -> Self {
        Self(i)
    }
}

/// RSX / Builder 选中回调。
#[derive(Clone)]
pub struct ChartSelect(pub Rc<dyn Fn(usize)>);

impl<F: Fn(usize) + 'static> From<F> for ChartSelect {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}

pub(crate) fn rows_from_values_labels(values: &[f32], labels: &[String]) -> Vec<ChartDatum> {
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let label = labels
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("{}", i + 1));
            ChartDatum::new(label, v)
        })
        .collect()
}

pub(crate) fn datum_hover_text(rows: &[ChartDatum], index: usize) -> Option<String> {
    let r = rows.get(index)?;
    let name = if !r.label.is_empty() {
        r.label.clone()
    } else if let Some(t) = r.time {
        super::scale::format_time_label(t, 86_400_000)
    } else {
        format!("{}", index + 1)
    };
    Some(format!("{}  {:.2}", name, r.value))
}

pub(crate) fn collect_rows<I, T>(data: I) -> Vec<ChartDatum>
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    data.into_iter().map(Into::into).collect()
}
