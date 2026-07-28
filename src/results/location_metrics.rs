use crate::runners::LocationMetrics;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy)]
pub(super) enum MetricFormat {
    Percentage,
    Decimal,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MetricDescriptor {
    pub label: &'static str,
    pub format: MetricFormat,
    pub kind: MetricKind,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum MetricKind {
    DestinationRecall,
    ExactTokenRecall,
    StrictPrecision,
    PolicyAdjustedPrecision,
    ExactSetRate,
    ExtraResultBurden,
}

impl MetricKind {
    fn index(self) -> usize {
        self as usize
    }
}

pub(super) const METRIC_DESCRIPTORS: [MetricDescriptor; 6] = [
    MetricDescriptor {
        label: "Destination recall",
        format: MetricFormat::Percentage,
        kind: MetricKind::DestinationRecall,
    },
    MetricDescriptor {
        label: "Exact-token recall",
        format: MetricFormat::Percentage,
        kind: MetricKind::ExactTokenRecall,
    },
    MetricDescriptor {
        label: "Strict precision",
        format: MetricFormat::Percentage,
        kind: MetricKind::StrictPrecision,
    },
    MetricDescriptor {
        label: "Policy-adjusted precision",
        format: MetricFormat::Percentage,
        kind: MetricKind::PolicyAdjustedPrecision,
    },
    MetricDescriptor {
        label: "Exact-set case rate",
        format: MetricFormat::Percentage,
        kind: MetricKind::ExactSetRate,
    },
    MetricDescriptor {
        label: "Extras/success",
        format: MetricFormat::Decimal,
        kind: MetricKind::ExtraResultBurden,
    },
];

#[derive(Debug, Default, Clone)]
pub(super) struct ProfileLocationMetrics {
    pub micro: LocationMetrics,
    pub case_macro: MetricAverageSet,
    pub cases: usize,
}

impl ProfileLocationMetrics {
    pub fn add_case(&mut self, metrics: &LocationMetrics) -> Result<()> {
        self.micro.checked_merge(metrics)?;
        self.micro.validate()?;
        self.case_macro.add(metric_rates(metrics));
        self.cases = self
            .cases
            .checked_add(1)
            .context("location case count overflow")?;
        Ok(())
    }

    pub fn merge(&mut self, other: &Self) -> Result<()> {
        self.micro.checked_merge(&other.micro)?;
        self.micro.validate()?;
        self.case_macro.merge(&other.case_macro);
        self.cases = self
            .cases
            .checked_add(other.cases)
            .context("profile location case count overflow")?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct MetricRates {
    pub destination_recall: Option<f64>,
    pub exact_token_recall: Option<f64>,
    pub strict_precision: Option<f64>,
    pub policy_adjusted_precision: Option<f64>,
    pub exact_set_rate: Option<f64>,
    pub extra_result_burden: Option<f64>,
}

impl MetricRates {
    pub fn get(self, kind: MetricKind) -> Option<f64> {
        match kind {
            MetricKind::DestinationRecall => self.destination_recall,
            MetricKind::ExactTokenRecall => self.exact_token_recall,
            MetricKind::StrictPrecision => self.strict_precision,
            MetricKind::PolicyAdjustedPrecision => self.policy_adjusted_precision,
            MetricKind::ExactSetRate => self.exact_set_rate,
            MetricKind::ExtraResultBurden => self.extra_result_burden,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(super) struct MetricAverageSet {
    values: [RateAverage; 6],
}

impl MetricAverageSet {
    pub fn add(&mut self, rates: MetricRates) {
        for descriptor in METRIC_DESCRIPTORS {
            self.values[descriptor.kind.index()].add(rates.get(descriptor.kind));
        }
    }

    pub fn merge(&mut self, other: &Self) {
        for (average, other_average) in self.values.iter_mut().zip(other.values) {
            average.merge(other_average);
        }
    }

    pub fn rates(&self) -> MetricRates {
        MetricRates {
            destination_recall: self.values[MetricKind::DestinationRecall.index()].value(),
            exact_token_recall: self.values[MetricKind::ExactTokenRecall.index()].value(),
            strict_precision: self.values[MetricKind::StrictPrecision.index()].value(),
            policy_adjusted_precision: self.values[MetricKind::PolicyAdjustedPrecision.index()]
                .value(),
            exact_set_rate: self.values[MetricKind::ExactSetRate.index()].value(),
            extra_result_burden: self.values[MetricKind::ExtraResultBurden.index()].value(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct RateAverage {
    sum: f64,
    count: usize,
}

impl RateAverage {
    fn add(&mut self, value: Option<f64>) {
        if let Some(value) = value {
            self.sum += value;
            self.count += 1;
        }
    }

    fn merge(&mut self, other: Self) {
        self.sum += other.sum;
        self.count += other.count;
    }

    fn value(self) -> Option<f64> {
        (self.count > 0).then(|| self.sum / self.count as f64)
    }
}

pub(super) fn metric_rates(metrics: &LocationMetrics) -> MetricRates {
    let required = metrics.true_positives + metrics.false_negatives;
    let strict_precision_denominator = metrics.true_positives
        + metrics.returned_locations.policy_allowed
        + metrics.false_positives;
    let policy_precision_denominator = metrics.true_positives + metrics.false_positives;
    MetricRates {
        destination_recall: ratio(metrics.true_positives, required),
        exact_token_recall: ratio(metrics.range_quality.exact_token, required),
        strict_precision: ratio(metrics.true_positives, strict_precision_denominator),
        policy_adjusted_precision: ratio(metrics.true_positives, policy_precision_denominator),
        exact_set_rate: ratio(metrics.exact_set_cases, metrics.cases),
        extra_result_burden: ratio(metrics.successful_query_extras, metrics.successful_queries),
    }
}

pub(super) fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| numerator as f64 / denominator as f64)
}
