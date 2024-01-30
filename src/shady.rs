use anyhow::Result;

use crate::{
    check::LabelDiff,
    dataset::{Datapoint, DynLabelConfig, YoloBB, YoloLabel},
};
use std::{fmt::Display, fs, path::PathBuf};

#[derive(Debug, Clone, PartialEq)]
enum Model {
    YoloV7,
    YoloV8,
}

#[derive(Debug, Clone, PartialEq)]
enum LabelType {
    ValueSuit,
    Value,
    Suit,
}
impl LabelType {
    fn cast(&self, label: &mut Vec<YoloBB>) {
        let max_usize = match self {
            LabelType::ValueSuit => 13 * 4,
            LabelType::Value => 13,
            LabelType::Suit => 4,
        };
        for l in label {
            l.class_num = l.class_num % max_usize
        }
    }
}

#[derive(Debug, Clone)]
enum Augmentation {
    PreciseBBox,
    InpreciseBBox,
}

#[derive(Debug, Clone)]
struct Metadata {
    model: Model,
    label_type: LabelType,
    augmentation: Augmentation,
}

struct Dataset {
    name: String,
    points: Vec<Datapoint>,
    metadata: Metadata,
    config: DynLabelConfig,
}

impl Dataset {
    fn from_dir(subdir: &PathBuf) -> Result<Self> {
        let dirname = subdir.file_name().unwrap().to_str().unwrap();
        let model = if dirname.contains("yolov7") {
            Model::YoloV7
        } else {
            Model::YoloV8
        };
        let label_type = if dirname.contains("yolov7") {
            LabelType::Value
        } else if dirname.contains("only_suit") {
            LabelType::Suit
        } else {
            LabelType::ValueSuit
        };
        let augmentation = if dirname.contains("rot") {
            Augmentation::InpreciseBBox
        } else {
            Augmentation::PreciseBBox
        };

        let points = Dataset::load_points(subdir)?;
        let config_filename = match label_type {
            LabelType::Value => "labels_13.toml",
            LabelType::ValueSuit => "labels_52.toml",
            LabelType::Suit => "labels_suit.toml",
        };
        let name = dirname.to_string();
        let config = DynLabelConfig::load_from_file(config_filename).unwrap();
        let metadata = Metadata {
            model,
            label_type,
            augmentation,
        };
        println!("{metadata:?}");
        Ok(Dataset {
            name,
            points,
            metadata,
            config,
        })
    }

    fn load_points(labels_dir: &PathBuf) -> Result<Vec<Datapoint>> {
        let mut data = vec![];
        let jpg_glob = format!("{}/*.txt", labels_dir.to_str().unwrap());
        let mut paths: Vec<_> = glob::glob(&jpg_glob)?.filter_map(Result::ok).collect();
        alphanumeric_sort::sort_path_slice(&mut paths);
        for img_src in paths.into_iter() {
            data.push(Datapoint::new(img_src, labels_dir.to_path_buf()))
        }
        if data.is_empty() {
            return Err(anyhow::anyhow!(
                "Dataset is empty. You need to put .txt files in {labels_dir:?}"
            ));
        }
        Ok(data)
    }
}

impl Display for Dataset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub struct ShadyFinder {
    normal_dataset: Dataset,
    augmented: Vec<Dataset>,
}

struct MultipleLabels {
    normal_index: usize,
    labels: Vec<(YoloLabel, Metadata)>,
}

impl MultipleLabels {
    fn diffs_to_normal(&self) -> impl Iterator<Item = (LabelDiff, &Metadata)> {
        self.labels
            .iter()
            .enumerate()
            .filter_map(|(i, label)| {
                if i != self.normal_index {
                    Some(label)
                } else {
                    None
                }
            })
            .map(|(label, meta)| {
                (
                    LabelDiff::new(&self.labels[self.normal_index].0, label, 0.95),
                    meta,
                )
            })
    }
    fn labels_for<'b, F: Fn(&Metadata) -> bool + 'b>(
        &'b self,
        filter: F,
    ) -> impl Iterator<Item = &'b YoloLabel> {
        self.labels
            .iter()
            .filter_map(move |(label, meta)| if filter(meta) { Some(label) } else { None })
    }

    fn suggested_label(&self) -> YoloLabel {
        // take the yolov7 labels with only values as the base
        // if all yolov8 agree with the suit, then take that
        let yolov7_base = self
            .labels_for(|m| m.model == Model::YoloV7)
            .next()
            .expect("should have yolov7 label");
        let only_suit = self.labels_for(|m| m.label_type == LabelType::Suit);
        // let mega_suit = all_suit.into_iter().flatten();
        // This is asymetric, because otherwise it will be too many computations
        let mut suggestions = vec![];
        for suit_label in only_suit {
            let diff = LabelDiff::new(yolov7_base, suit_label, 0.95);
            let mut label = vec![];
            for (value_box, suit_box) in diff.intersection() {
                let mut value_suit: YoloBB = *value_box;
                // TODO abstract this transformation computation away, we do this multiple times in this codebase.
                // Should be somehow specified in the config
                let suit_class = suit_box.class_num % 4;
                let value_class = value_box.class_num % 13;
                value_suit.class_num = suit_class * 13 + value_class;
                label.push(value_suit)
            }
            suggestions.push(label);
        }
        suggestions
            .into_iter()
            .max_by_key(|l| l.len())
            .expect("there should be a only suit model for suggested_label")
    }
}

impl ShadyFinder {
    pub fn new(data_dir: &PathBuf) -> Result<Self> {
        let mut normal = None;
        let mut augmented = vec![];
        for entry in fs::read_dir(data_dir).unwrap() {
            let entry = entry?;
            let subdir = entry.path();
            if !subdir.is_dir() {
                continue;
            }
            let dirname = subdir.file_name().unwrap().to_str().unwrap();

            if dirname == "normal" {
                normal = Some(Dataset::from_dir(&subdir)?);
            } else {
                augmented.push(Dataset::from_dir(&subdir)?);
            }
        }
        Ok(ShadyFinder {
            normal_dataset: normal.expect("there is no folder called normal"),
            augmented,
        })
    }

    fn read_all_labels(&self, i: usize) -> Result<MultipleLabels> {
        let normal = self.normal_dataset.points[i].load_label()?;
        let mut labels = vec![(normal, self.normal_dataset.metadata.clone())];
        for aug_data in &self.augmented {
            let mut label = aug_data.points[i].load_label()?;
            aug_data.metadata.label_type.cast(&mut label);
            // let diff = LabelDiff::new(&normal, &augmented, 0.97);
            labels.push((label, aug_data.metadata.clone()));
        }
        Ok(MultipleLabels {
            normal_index: 0,
            labels,
        })
    }

    // pub fn print_shady(&self) -> Result<()> {
    //     // iterate all data in normal
    //     // assumes that the datasets describe identical images and are hence equal size
    //     for i in 0..self.normal_dataset.points.len() {
    //         let labels = self.read_all_labels(i)?;
    //         for ((diff, _meta), aug_data) in labels.diffs_to_normal().zip(&self.augmented) {
    //             if !diff.are_equal() {
    //                 println!(
    //                     "{i:>8} {} {}",
    //                     aug_data.name,
    //                     diff.summary(&self.normal_dataset.config, &aug_data.config)
    //                 )
    //             }
    //         }
    //     }
    //     // for (i, point) in self.normal_dataset.points.iter().enumerate() {
    //     //     let normal = point.load_label()?;
    //     //     for aug_data in &self.augmented {
    //     //         let augmented = aug_data.points[i].load_label()?;
    //     //         // TODO this clone is super unneccessary
    //     //         let diff = LabelDiff::new(normal.clone(), augmented, 0.97);
    //     //         if !diff.are_equal() {
    //     //             let summary = diff.summary(&self.normal_dataset.config, &aug_data.config);
    //     //             println!("{i:>8} normal vs {aug_data}: {}", summary);
    //     //         }
    //     //     }
    //     // }
    //     Ok(())
    // }

    pub fn suggest_labels(&self, prefix: &str) -> Result<()> {
        for i in 0..self.normal_dataset.points.len() {
            let diff = self.read_all_labels(i)?;
            let suggested: YoloLabel = diff.suggested_label();

            let relabel_point = self.normal_dataset.points[i].add_prefix(prefix);
            relabel_point.save_label(suggested)?;
        }
        Ok(())
    }
}
