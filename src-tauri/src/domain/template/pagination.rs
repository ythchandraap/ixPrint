use super::FooterMode;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Band {
    Header,
    Row,
    Footer,
}

/// One printed page of a list: which rows it holds and where the footer goes.
#[derive(Debug, PartialEq)]
pub(crate) struct ListPage {
    pub(crate) rows: std::ops::Range<usize>,
    /// Top of the footer band in mm, if the footer is on this page.
    pub(crate) footer_top: Option<f32>,
    pub(crate) height_mm: f32,
}

/// Splits `n` rows into pages (pure layout math).
pub(crate) fn paginate(
    n: usize,
    page_h: f32,
    header: f32,
    row: f32,
    footer_h: f32,
    mode: FooterMode,
    roll: bool,
) -> Vec<ListPage> {
    const EPS: f32 = 0.01;
    if roll {
        let height = header + n as f32 * row + footer_h;
        return vec![ListPage {
            rows: 0..n,
            footer_top: Some(header + n as f32 * row),
            height_mm: height,
        }];
    }
    let every = mode == FooterMode::EveryPage;
    let bottom = page_h - if every { footer_h } else { 0.0 };
    let per_page = (((bottom - header) + EPS) / row).floor().max(1.0) as usize;
    let mut pages = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + per_page).min(n);
        pages.push(ListPage {
            rows: start..end,
            footer_top: every.then_some(page_h - footer_h),
            height_mm: page_h,
        });
        start = end;
        if start >= n {
            break;
        }
    }
    if !every && footer_h > 0.0 {
        let last = pages.last_mut().unwrap();
        let after = header + last.rows.len() as f32 * row;
        if after + footer_h <= page_h + EPS {
            last.footer_top = Some(after);
        } else {
            // No room left: the footer gets its own page, under the header.
            pages.push(ListPage {
                rows: n..n,
                footer_top: Some(header),
                height_mm: page_h,
            });
        }
    }
    pages
}
