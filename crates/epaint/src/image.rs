use emath::Vec2;

use crate::{Color32, textures::TextureOptions};
use std::sync::Arc;

/// An image stored in RAM.
///
/// To load an image file, see [`ColorImage::from_rgba_unmultiplied`].
///
/// In order to paint the image on screen, you first need to convert it to
///
/// See also: [`ColorImage`], [`FontImage`].
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ImageData {
    /// RGBA image.
    Color(Arc<ColorImage>),

    /// Used for the font texture.
    Font(FontImage),
}

impl ImageData {
    pub fn size(&self) -> [usize; 2] {
        match self {
            Self::Color(image) => image.size,
            Self::Font(image) => image.size,
        }
    }

    pub fn width(&self) -> usize {
        self.size()[0]
    }

    pub fn height(&self) -> usize {
        self.size()[1]
    }

    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::Color(_) | Self::Font(_) => 4,
        }
    }
}

// ----------------------------------------------------------------------------

/// A 2D RGBA color image in RAM.
#[derive(Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ColorImage {
    /// width, height in texels.
    pub size: [usize; 2],

    /// Size of the original SVG image (if any), or just the texel size of the image.
    pub source_size: Vec2,

    /// The pixels, row by row, from top to bottom.
    pub pixels: Vec<Color32>,
}

impl ColorImage {
    /// Create an image filled with the given color.
    pub fn new(size: [usize; 2], pixels: Vec<Color32>) -> Self {
        debug_assert!(
            size[0] * size[1] == pixels.len(),
            "size: {size:?}, pixels.len(): {}",
            pixels.len()
        );
        Self {
            size,
            source_size: Vec2::new(size[0] as f32, size[1] as f32),
            pixels,
        }
    }

    /// Create an image filled with the given color.
    pub fn filled(size: [usize; 2], color: Color32) -> Self {
        Self {
            size,
            source_size: Vec2::new(size[0] as f32, size[1] as f32),
            pixels: vec![color; size[0] * size[1]],
        }
    }

    /// Create a [`ColorImage`] from flat un-multiplied RGBA data.
    ///
    /// This is usually what you want to use after having loaded an image file.
    ///
    /// Panics if `size[0] * size[1] * 4 != rgba.len()`.
    ///
    /// ## Example using the [`image`](crates.io/crates/image) crate:
    /// ``` ignore
    /// fn load_image_from_path(path: &std::path::Path) -> Result<egui::ColorImage, image::ImageError> {
    ///     let image = image::io::Reader::open(path)?.decode()?;
    ///     let size = [image.width() as _, image.height() as _];
    ///     let image_buffer = image.to_rgba8();
    ///     let pixels = image_buffer.as_flat_samples();
    ///     Ok(egui::ColorImage::from_rgba_unmultiplied(
    ///         size,
    ///         pixels.as_slice(),
    ///     ))
    /// }
    ///
    /// fn load_image_from_memory(image_data: &[u8]) -> Result<ColorImage, image::ImageError> {
    ///     let image = image::load_from_memory(image_data)?;
    ///     let size = [image.width() as _, image.height() as _];
    ///     let image_buffer = image.to_rgba8();
    ///     let pixels = image_buffer.as_flat_samples();
    ///     Ok(ColorImage::from_rgba_unmultiplied(
    ///         size,
    ///         pixels.as_slice(),
    ///     ))
    /// }
    /// ```
    pub fn from_rgba_unmultiplied(size: [usize; 2], rgba: &[u8]) -> Self {
        assert_eq!(
            size[0] * size[1] * 4,
            rgba.len(),
            "size: {:?}, rgba.len(): {}",
            size,
            rgba.len()
        );
        let pixels = rgba
            .chunks_exact(4)
            .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect();
        Self::new(size, pixels)
    }

    pub fn from_rgba_premultiplied(size: [usize; 2], rgba: &[u8]) -> Self {
        assert_eq!(
            size[0] * size[1] * 4,
            rgba.len(),
            "size: {:?}, rgba.len(): {}",
            size,
            rgba.len()
        );
        let pixels = rgba
            .chunks_exact(4)
            .map(|p| Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]))
            .collect();
        Self::new(size, pixels)
    }

    /// Create a [`ColorImage`] from flat opaque gray data.
    ///
    /// Panics if `size[0] * size[1] != gray.len()`.
    pub fn from_gray(size: [usize; 2], gray: &[u8]) -> Self {
        assert_eq!(
            size[0] * size[1],
            gray.len(),
            "size: {:?}, gray.len(): {}",
            size,
            gray.len()
        );
        let pixels = gray.iter().map(|p| Color32::from_gray(*p)).collect();
        Self::new(size, pixels)
    }

    /// Alternative method to `from_gray`.
    /// Create a [`ColorImage`] from iterator over flat opaque gray data.
    ///
    /// Panics if `size[0] * size[1] != gray_iter.len()`.
    #[doc(alias = "from_grey_iter")]
    pub fn from_gray_iter(size: [usize; 2], gray_iter: impl Iterator<Item = u8>) -> Self {
        let pixels: Vec<_> = gray_iter.map(Color32::from_gray).collect();
        assert_eq!(
            size[0] * size[1],
            pixels.len(),
            "size: {:?}, pixels.len(): {}",
            size,
            pixels.len()
        );
        Self::new(size, pixels)
    }

    /// A view of the underlying data as `&[u8]`
    #[cfg(feature = "bytemuck")]
    pub fn as_raw(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }

    /// A view of the underlying data as `&mut [u8]`
    #[cfg(feature = "bytemuck")]
    pub fn as_raw_mut(&mut self) -> &mut [u8] {
        bytemuck::cast_slice_mut(&mut self.pixels)
    }

    /// Create a [`ColorImage`] from flat RGB data.
    ///
    /// This is what you want to use after having loaded an image file (and if
    /// you are ignoring the alpha channel - considering it to always be 0xff)
    ///
    /// Panics if `size[0] * size[1] * 3 != rgb.len()`.
    pub fn from_rgb(size: [usize; 2], rgb: &[u8]) -> Self {
        assert_eq!(
            size[0] * size[1] * 3,
            rgb.len(),
            "size: {:?}, rgb.len(): {}",
            size,
            rgb.len()
        );
        let pixels = rgb
            .chunks_exact(3)
            .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
            .collect();
        Self::new(size, pixels)
    }

    /// An example color image, useful for tests.
    pub fn example() -> Self {
        let width = 128;
        let height = 64;
        let mut img = Self::filled([width, height], Color32::TRANSPARENT);
        for y in 0..height {
            for x in 0..width {
                let h = x as f32 / width as f32;
                let s = 1.0;
                let v = 1.0;
                let a = y as f32 / height as f32;
                img[(x, y)] = crate::Hsva { h, s, v, a }.into();
            }
        }
        img
    }

    /// Set the source size of e.g. the original SVG image.
    #[inline]
    pub fn with_source_size(mut self, source_size: Vec2) -> Self {
        self.source_size = source_size;
        self
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.size[0]
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.size[1]
    }

    /// Create a new image from a patch of the current image.
    ///
    /// This method is especially convenient for screenshotting a part of the app
    /// since `region` can be interpreted as screen coordinates of the entire screenshot if `pixels_per_point` is provided for the native application.
    /// The floats of [`emath::Rect`] are cast to usize, rounding them down in order to interpret them as indices to the image data.
    ///
    /// Panics if `region.min.x > region.max.x || region.min.y > region.max.y`, or if a region larger than the image is passed.
    pub fn region(&self, region: &emath::Rect, pixels_per_point: Option<f32>) -> Self {
        let pixels_per_point = pixels_per_point.unwrap_or(1.0);
        let min_x = (region.min.x * pixels_per_point) as usize;
        let max_x = (region.max.x * pixels_per_point) as usize;
        let min_y = (region.min.y * pixels_per_point) as usize;
        let max_y = (region.max.y * pixels_per_point) as usize;
        assert!(
            min_x <= max_x && min_y <= max_y,
            "Screenshot region is invalid: {region:?}"
        );
        let width = max_x - min_x;
        let height = max_y - min_y;
        let mut output = Vec::with_capacity(width * height);
        let row_stride = self.size[0];

        for row in min_y..max_y {
            output.extend_from_slice(
                &self.pixels[row * row_stride + min_x..row * row_stride + max_x],
            );
        }
        Self::new([width, height], output)
    }
}

impl std::ops::Index<(usize, usize)> for ColorImage {
    type Output = Color32;

    #[inline]
    fn index(&self, (x, y): (usize, usize)) -> &Color32 {
        let [w, h] = self.size;
        assert!(x < w && y < h, "x: {x}, y: {y}, w: {w}, h: {h}");
        &self.pixels[y * w + x]
    }
}

impl std::ops::IndexMut<(usize, usize)> for ColorImage {
    #[inline]
    fn index_mut(&mut self, (x, y): (usize, usize)) -> &mut Color32 {
        let [w, h] = self.size;
        assert!(x < w && y < h, "x: {x}, y: {y}, w: {w}, h: {h}");
        &mut self.pixels[y * w + x]
    }
}

impl From<ColorImage> for ImageData {
    #[inline(always)]
    fn from(image: ColorImage) -> Self {
        Self::Color(Arc::new(image))
    }
}

impl From<Arc<ColorImage>> for ImageData {
    #[inline]
    fn from(image: Arc<ColorImage>) -> Self {
        Self::Color(image)
    }
}

impl std::fmt::Debug for ColorImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorImage")
            .field("size", &self.size)
            .field("pixel-count", &self.pixels.len())
            .finish_non_exhaustive()
    }
}

// ----------------------------------------------------------------------------

/// A single-channel image designed for the font texture.
///
/// Each value represents "coverage", i.e. how much a texel is covered by a character.
///
/// This is roughly interpreted as the opacity of a white image.
#[derive(Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct FontImage {
    /// width, height
    pub size: [usize; 2],

    /// The coverage value.
    ///
    /// Often you want to use [`Self::srgba_pixels`] instead.
    pub pixels: Vec<f32>,
}

impl FontImage {
    pub fn new(size: [usize; 2]) -> Self {
        Self {
            size,
            pixels: vec![0.0; size[0] * size[1]],
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.size[0]
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.size[1]
    }

    /// Returns the textures as `sRGBA` premultiplied pixels, row by row, top to bottom.
    ///
    /// `gamma` should normally be set to `None`.
    ///
    /// If you are having problems with text looking skinny and pixelated, try using a low gamma, e.g. `0.4`.
    #[inline]
    pub fn srgba_pixels(&self, gamma: Option<f32>) -> impl ExactSizeIterator<Item = Color32> + '_ {
        // This whole function is less than rigorous.
        // Ideally we should do this in a shader instead, and use different computations
        // for different text colors.
        // See https://hikogui.org/2022/10/24/the-trouble-with-anti-aliasing.html for an in-depth analysis.
        self.pixels.iter().map(move |coverage| {
            let alpha = if let Some(gamma) = gamma {
                coverage.powf(gamma)
            } else {
                // alpha = coverage * coverage; // recommended by the article for WHITE text (using linear blending)

                // The following is recommended by the article for BLACK text (using linear blending).
                // Very similar to a gamma of 0.5, but produces sharper text.
                // In practice it works well for all text colors (better than a gamma of 0.5, for instance).
                // See https://www.desmos.com/calculator/w0ndf5blmn for a visual comparison.
                2.0 * coverage - coverage * coverage
            };
            Color32::from_white_alpha(ecolor::linear_u8_from_linear_f32(alpha))
        })
    }

    /// Clone a sub-region as a new image.
    pub fn region(&self, [x, y]: [usize; 2], [w, h]: [usize; 2]) -> Self {
        assert!(
            x + w <= self.width(),
            "x + w should be <= self.width(), but x: {}, w: {}, width: {}",
            x,
            w,
            self.width()
        );
        assert!(
            y + h <= self.height(),
            "y + h should be <= self.height(), but y: {}, h: {}, height: {}",
            y,
            h,
            self.height()
        );

        let mut pixels = Vec::with_capacity(w * h);
        for y in y..y + h {
            let offset = y * self.width() + x;
            pixels.extend(&self.pixels[offset..(offset + w)]);
        }
        assert_eq!(
            pixels.len(),
            w * h,
            "pixels.len should be w * h, but got {}",
            pixels.len()
        );
        Self {
            size: [w, h],
            pixels,
        }
    }
}

impl std::ops::Index<(usize, usize)> for FontImage {
    type Output = f32;

    #[inline]
    fn index(&self, (x, y): (usize, usize)) -> &f32 {
        let [w, h] = self.size;
        assert!(x < w && y < h, "x: {x}, y: {y}, w: {w}, h: {h}");
        &self.pixels[y * w + x]
    }
}

impl std::ops::IndexMut<(usize, usize)> for FontImage {
    #[inline]
    fn index_mut(&mut self, (x, y): (usize, usize)) -> &mut f32 {
        let [w, h] = self.size;
        assert!(x < w && y < h, "x: {x}, y: {y}, w: {w}, h: {h}");
        &mut self.pixels[y * w + x]
    }
}

impl From<FontImage> for ImageData {
    #[inline(always)]
    fn from(image: FontImage) -> Self {
        Self::Font(image)
    }
}

// ----------------------------------------------------------------------------

/// A change to an image.
///
/// Either a whole new image, or an update to a rectangular region of it.
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[must_use = "The painter must take care of this"]
pub struct ImageDelta {
    /// What to set the texture to.
    ///
    /// If [`Self::pos`] is `None`, this describes the whole texture.
    ///
    /// If [`Self::pos`] is `Some`, this describes a patch of the whole image starting at [`Self::pos`].
    pub image: ImageData,

    pub options: TextureOptions,

    /// If `None`, set the whole texture to [`Self::image`].
    ///
    /// If `Some(pos)`, update a sub-region of an already allocated texture with the patch in [`Self::image`].
    pub pos: Option<[usize; 2]>,
}

impl ImageDelta {
    /// Update the whole texture.
    pub fn full(image: impl Into<ImageData>, options: TextureOptions) -> Self {
        Self {
            image: image.into(),
            options,
            pos: None,
        }
    }

    /// Update a sub-region of an existing texture.
    pub fn partial(pos: [usize; 2], image: impl Into<ImageData>, options: TextureOptions) -> Self {
        Self {
            image: image.into(),
            options,
            pos: Some(pos),
        }
    }

    /// Is this affecting the whole texture?
    /// If `false`, this is a partial (sub-region) update.
    pub fn is_whole(&self) -> bool {
        self.pos.is_none()
    }
}
