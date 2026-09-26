use image::{GenericImageView, ImageDecoder, ImageFormat, ImageReader};
use pdfium_render::prelude::*;
use std::path::Path;

const A4_SHORT_PT: f32 = 210.0 * 72.0 / 25.4;
const A4_LONG_PT: f32 = 297.0 * 72.0 / 25.4;

struct ImageFit {
    page_width: f32,
    page_height: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

pub fn convert(source: &Path, output: &Path) -> Result<(), String> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
        return Err("Choose a PNG or JPG/JPEG image".into());
    }
    let reader = ImageReader::open(source)
        .map_err(|_| "The selected image could not be read".to_string())?
        .with_guessed_format()
        .map_err(|_| "The selected image format could not be identified".to_string())?;
    let format = reader
        .format()
        .ok_or("The selected image format could not be identified")?;
    let expected = if extension == "png" {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    };
    if format != expected {
        return Err("The image content does not match its .png or .jpg extension".into());
    }
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "The selected image is damaged or unsupported".to_string())?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let (image_width, image_height) = decoder.dimensions();
    if image_width == 0 || image_height == 0 || u64::from(image_width) * u64::from(image_height) > 100_000_000 {
        return Err("Image dimensions are not supported (maximum 100 megapixels)".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(30_000);
    limits.max_image_height = Some(30_000);
    limits.max_alloc = Some(400 * 1024 * 1024);
    decoder
        .set_limits(limits)
        .map_err(|_| "Image dimensions exceed the safe conversion limit".to_string())?;
    let mut image = image::DynamicImage::from_decoder(decoder)
        .map_err(|_| "The selected image is damaged or unsupported".to_string())?;
    image.apply_orientation(orientation);
    let (image_width, image_height) = image.dimensions();
    let fit = fit_image(image_width, image_height);

    let pdfium = crate::infrastructure::pdf::layout_engine::pdfium()?;
    let mut document = pdfium.create_new_pdf().map_err(|error| error.to_string())?;
    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::Custom(
            PdfPoints::new(fit.page_width),
            PdfPoints::new(fit.page_height),
        ))
        .map_err(|error| error.to_string())?;
    page.objects_mut()
        .create_image_object(
            PdfPoints::new(fit.x),
            PdfPoints::new(fit.y),
            &image,
            Some(PdfPoints::new(fit.width)),
            Some(PdfPoints::new(fit.height)),
        )
        .map_err(|_| "Image could not be added to its PDF page".to_string())?;
    page.regenerate_content()
        .map_err(|_| "Image PDF could not be finalized".to_string())?;
    document
        .save_to_file(output)
        .map_err(|_| "Image PDF could not be saved".to_string())
}

fn fit_image(image_width: u32, image_height: u32) -> ImageFit {
    let (page_width, page_height) = if image_width > image_height {
        (A4_LONG_PT, A4_SHORT_PT)
    } else {
        (A4_SHORT_PT, A4_LONG_PT)
    };
    let scale = (page_width / image_width as f32).min(page_height / image_height as f32);
    let width = image_width as f32 * scale;
    let height = image_height as f32 * scale;
    ImageFit {
        page_width,
        page_height,
        x: (page_width - width) / 2.0,
        y: (page_height - height) / 2.0,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_landscape_image_without_cropping_or_changing_ratio() {
        let fit = fit_image(1600, 900);
        assert!(fit.page_width > fit.page_height);
        assert!(fit.x >= 0.0 && fit.y >= 0.0);
        assert!(fit.x + fit.width <= fit.page_width + 0.01);
        assert!(fit.y + fit.height <= fit.page_height + 0.01);
        assert!((fit.width / fit.height - 1600.0 / 900.0).abs() < 0.001);
    }

    #[test]
    fn fits_portrait_image_on_a4_portrait_page() {
        let fit = fit_image(900, 1600);
        assert!(fit.page_width < fit.page_height);
        assert!(fit.x >= 0.0 && fit.y >= 0.0);
        assert!(fit.x + fit.width <= fit.page_width + 0.01);
        assert!(fit.y + fit.height <= fit.page_height + 0.01);
        assert!((fit.width / fit.height - 900.0 / 1600.0).abs() < 0.001);
    }
}
