use std::path::PathBuf;

use crate::dataset::{Dataset, DynLabelConfig};
use itertools::iproduct;

use crate::dataset::{YoloBB, YoloLabel};

pub struct LabelDiff<'a, 'b> {
    pub a: &'a YoloLabel,
    pub b: &'b YoloLabel,
    pub thresh: f32,
}

impl<'a, 'b> LabelDiff<'a, 'b> {
    pub fn class_differences_itersection(&self) -> impl Iterator<Item = (&YoloBB, &YoloBB)> + '_ {
        // Only iterate the intersection, so the order doesn't matter
        self.intersection()
            .filter(|(a_bb, b_bb)| a_bb.class_num != b_bb.class_num)
    }
    pub fn intersection(&self) -> impl Iterator<Item = (&YoloBB, &YoloBB)> + '_ {
        // Only iterate the intersection, so the order doesn't matter
        iproduct!(self.a, self.b).filter(|(a_bb, b_bb)| a_bb.iou(b_bb) > self.thresh)
    }
    pub fn only_in_a(&self) -> impl Iterator<Item = &YoloBB> + '_ {
        self.a
            .iter()
            .filter(|a| self.b.iter().all(|b| a.iou(b) < self.thresh))
    }
    pub fn only_in_b(&self) -> impl Iterator<Item = &YoloBB> + '_ {
        self.b
            .iter()
            .filter(|b| self.a.iter().all(|a| b.iou(a) < self.thresh))
    }
    pub fn are_equal(&self) -> bool {
        self.only_in_a().count() == 0
            && self.only_in_b().count() == 0
            && self.class_differences_itersection().count() == 0
    }

    pub fn new(a: &'a YoloLabel, b: &'b YoloLabel, thresh: f32) -> Self {
        Self { a, b, thresh }
    }

    pub fn summary(&self, a_config: &DynLabelConfig, b_config: &DynLabelConfig) -> String {
        let only_a = self
            .only_in_a()
            .map(|bb| bb.class(a_config).name)
            .collect::<Vec<String>>()
            .join(",");
        let only_b = self
            .only_in_b()
            .map(|bb| bb.class(b_config).name)
            .collect::<Vec<String>>()
            .join(",");
        let diffs = self
            .class_differences_itersection()
            .map(|(a, b)| format!("{} != {}", a.class(a_config).name, b.class(b_config).name))
            .collect::<Vec<String>>()
            .join(",");
        format!("only a {only_a}; only b {only_b}; diffs {diffs}")
    }
}
pub fn check_differences(
    path: PathBuf,
    prefix: &str,
    prefix_relabel: &str,
    config: PathBuf,
    config_relabel: PathBuf,
) -> anyhow::Result<()> {
    let dataset = Dataset::with_prefix(&path, prefix)?;
    let relabel_dataset = Dataset::with_prefix(&path, prefix_relabel)?;
    let config = DynLabelConfig::load_from_file(config)?;
    let config_relabel = DynLabelConfig::load_from_file(config_relabel)?;

    for (i, (label, relabel)) in dataset
        .iter_data()
        .zip(relabel_dataset.iter_data())
        .enumerate()
    {
        let name = label.img_name();
        assert_eq!(name, relabel.img_name());
        let label = label.load_label()?;
        let relabel = relabel.load_label()?;
        let diff = LabelDiff::new(&label, &relabel, 0.99);
        if !diff.are_equal() {
            println!("{i:>8} {name}: {}", diff.summary(&config, &config_relabel))
        }
    }
    Ok(())
}
