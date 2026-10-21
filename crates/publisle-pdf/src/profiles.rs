//! Geometry measured after \maketitle in the pinned existing publisher classes.
//! Source worker: sha256:6ed5290f27fd1122e34b9b699974965f86ebbae91f1f96529c2bdb6142317727.
//! Values below are TeX points (72.27/in); PDF uses 72/in.
use crate::model::{Diagnostic, Result};

pub struct Profile {
    pub paper: [f32; 2],
    pub text_width: f32,
    pub left: [f32; 2],
    pub top: f32,
    pub bottom: f32,
    pub columns: usize,
    pub gutter: f32,
    pub body_size: f32,
    pub leading: f32,
    pub title_size: f32,
}
impl Profile {
    #[allow(clippy::too_many_arguments)]
    fn measured(
        paper: [f64; 2],
        text: [f64; 2],
        sides: [f64; 2],
        top: [f64; 3],
        columns: usize,
        gutter: f64,
        font: [f64; 2],
        title: f64,
    ) -> Self {
        let scale = 72. / 72.27;
        let top = (72.27 + top.iter().sum::<f64>()) * scale;
        Self {
            paper: paper.map(|n| (n * scale) as f32),
            text_width: (text[0] * scale) as f32,
            left: sides.map(|n| ((72.27 + n) * scale) as f32),
            top: top as f32,
            bottom: (paper[1] * scale - top - text[1] * scale) as f32,
            columns,
            gutter: (gutter * scale) as f32,
            body_size: (font[0] * scale) as f32,
            leading: (font[1] / font[0]) as f32,
            title_size: (title * scale) as f32,
        }
    }
}
pub fn profile(id: &str) -> Result<Profile> {
    Ok(match id {
        "article" | "article-arabic" => Profile::measured(
            [597.50787, 845.04684],
            [360., 595.80026],
            [46., 46.],
            [18., 12., 25.],
            1,
            10.,
            [10.95, 13.6],
            17.28,
        ),
        "ieee-journal" => Profile::measured(
            [614.295, 794.96999],
            [516., 696.],
            [-23.1225, -23.1225],
            [-52.93199, 12., 21.342],
            2,
            12.,
            [10., 12.],
            24.,
        ),
        "acm-journal" => Profile::measured(
            [614.295, 794.96999],
            [430.00462, 560.],
            [1.44614, 38.30426],
            [-3.87262, 13., 14.],
            1,
            10.,
            [9., 13.75],
            12.,
        ),
        "springer-journal" => Profile::measured(
            [597.50787, 845.04684],
            [372., 552.69478],
            [28.17613, 52.79176],
            [-19.72638, 5.5, 15.93355],
            1,
            14.22636,
            [10.03749, 12.045],
            17.06374,
        ),
        "elsevier-numeric" | "elsevier-author-date" => Profile::measured(
            [614.295, 794.96999],
            [345., 550.],
            [62., 62.],
            [16., 12., 25.],
            1,
            10.,
            [10., 18.],
            14.4,
        ),
        _ => {
            return Err(Diagnostic::new(
                "unsupported-template",
                format!("Native template {id} is unavailable."),
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_publisher_page_geometry() {
        let ieee = profile("ieee-journal").unwrap();
        assert!((ieee.paper[0] - 612.).abs() < 0.01);
        assert!((ieee.paper[1] - 792.).abs() < 0.01);
        assert_eq!(ieee.columns, 2);
        assert!((ieee.text_width - 514.0722).abs() < 0.01);
        assert_eq!(profile("acm-journal").unwrap().columns, 1);
        assert!((profile("elsevier-numeric").unwrap().paper[0] - 612.).abs() < 0.01);
    }
}
