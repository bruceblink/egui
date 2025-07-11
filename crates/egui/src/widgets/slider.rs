#![allow(clippy::needless_pass_by_value)] // False positives with `impl ToString`

use std::ops::RangeInclusive;

use crate::{
    Color32, DragValue, EventFilter, Key, Label, MINUS_CHAR_STR, NumExt as _, Pos2, Rangef, Rect,
    Response, Sense, TextStyle, TextWrapMode, Ui, Vec2, Widget, WidgetInfo, WidgetText, emath,
    epaint, lerp, pos2, remap, remap_clamp, style, style::HandleShape, vec2,
};

use super::drag_value::clamp_value_to_range;

// ----------------------------------------------------------------------------

type NumFormatter<'a> = Box<dyn 'a + Fn(f64, RangeInclusive<usize>) -> String>;
type NumParser<'a> = Box<dyn 'a + Fn(&str) -> Option<f64>>;

// ----------------------------------------------------------------------------

/// Combined into one function (rather than two) to make it easier
/// for the borrow checker.
type GetSetValue<'a> = Box<dyn 'a + FnMut(Option<f64>) -> f64>;

fn get(get_set_value: &mut GetSetValue<'_>) -> f64 {
    (get_set_value)(None)
}

fn set(get_set_value: &mut GetSetValue<'_>, value: f64) {
    (get_set_value)(Some(value));
}

// ----------------------------------------------------------------------------

#[derive(Clone)]
struct SliderSpec {
    logarithmic: bool,

    /// For logarithmic sliders, the smallest positive value we are interested in.
    /// 1 for integer sliders, maybe 1e-6 for others.
    smallest_positive: f64,

    /// For logarithmic sliders, the largest positive value we are interested in
    /// before the slider switches to `INFINITY`, if that is the higher end.
    /// Default: INFINITY.
    largest_finite: f64,
}

/// Specifies the orientation of a [`Slider`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SliderOrientation {
    Horizontal,
    Vertical,
}

/// Specifies how values in a [`Slider`] are clamped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SliderClamping {
    /// Values are not clamped.
    ///
    /// This means editing the value with the keyboard,
    /// or dragging the number next to the slider will always work.
    ///
    /// The actual slider part is always clamped though.
    Never,

    /// Users cannot enter new values that are outside the range.
    ///
    /// Existing values remain intact though.
    Edits,

    /// Always clamp values, even existing ones.
    #[default]
    Always,
}

/// Control a number with a slider.
///
/// The slider range defines the values you get when pulling the slider to the far edges.
/// By default all values are clamped to this range, even when not interacted with.
/// You can change this behavior by passing `false` to [`Slider::clamp_to_range`].
///
/// The range can include any numbers, and go from low-to-high or from high-to-low.
///
/// The slider consists of three parts: a slider, a value display, and an optional text.
/// The user can click the value display to edit its value. It can be turned off with `.show_value(false)`.
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// # let mut my_f32: f32 = 0.0;
/// ui.add(egui::Slider::new(&mut my_f32, 0.0..=100.0).text("My value"));
/// # });
/// ```
///
/// The default [`Slider`] size is set by [`crate::style::Spacing::slider_width`].
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct Slider<'a> {
    get_set_value: GetSetValue<'a>,
    range: RangeInclusive<f64>,
    spec: SliderSpec,
    clamping: SliderClamping,
    smart_aim: bool,
    show_value: bool,
    orientation: SliderOrientation,
    prefix: String,
    suffix: String,
    text: WidgetText,

    /// Sets the minimal step of the widget value
    step: Option<f64>,

    drag_value_speed: Option<f64>,
    min_decimals: usize,
    max_decimals: Option<usize>,
    custom_formatter: Option<NumFormatter<'a>>,
    custom_parser: Option<NumParser<'a>>,
    trailing_fill: Option<bool>,
    handle_shape: Option<HandleShape>,
    update_while_editing: bool,
}

impl<'a> Slider<'a> {
    /// Creates a new horizontal slider.
    ///
    /// The `value` given will be clamped to the `range`,
    /// unless you change this behavior with [`Self::clamping`].
    pub fn new<Num: emath::Numeric>(value: &'a mut Num, range: RangeInclusive<Num>) -> Self {
        let range_f64 = range.start().to_f64()..=range.end().to_f64();
        let slf = Self::from_get_set(range_f64, move |v: Option<f64>| {
            if let Some(v) = v {
                *value = Num::from_f64(v);
            }
            value.to_f64()
        });

        if Num::INTEGRAL { slf.integer() } else { slf }
    }

    pub fn from_get_set(
        range: RangeInclusive<f64>,
        get_set_value: impl 'a + FnMut(Option<f64>) -> f64,
    ) -> Self {
        Self {
            get_set_value: Box::new(get_set_value),
            range,
            spec: SliderSpec {
                logarithmic: false,
                smallest_positive: 1e-6,
                largest_finite: f64::INFINITY,
            },
            clamping: SliderClamping::default(),
            smart_aim: true,
            show_value: true,
            orientation: SliderOrientation::Horizontal,
            prefix: Default::default(),
            suffix: Default::default(),
            text: Default::default(),
            step: None,
            drag_value_speed: None,
            min_decimals: 0,
            max_decimals: None,
            custom_formatter: None,
            custom_parser: None,
            trailing_fill: None,
            handle_shape: None,
            update_while_editing: true,
        }
    }

    /// Control whether or not the slider shows the current value.
    /// Default: `true`.
    #[inline]
    pub fn show_value(mut self, show_value: bool) -> Self {
        self.show_value = show_value;
        self
    }

    /// Show a prefix before the number, e.g. "x: "
    #[inline]
    pub fn prefix(mut self, prefix: impl ToString) -> Self {
        self.prefix = prefix.to_string();
        self
    }

    /// Add a suffix to the number, this can be e.g. a unit ("°" or " m")
    #[inline]
    pub fn suffix(mut self, suffix: impl ToString) -> Self {
        self.suffix = suffix.to_string();
        self
    }

    /// Show a text next to the slider (e.g. explaining what the slider controls).
    #[inline]
    pub fn text(mut self, text: impl Into<WidgetText>) -> Self {
        self.text = text.into();
        self
    }

    #[inline]
    pub fn text_color(mut self, text_color: Color32) -> Self {
        self.text = self.text.color(text_color);
        self
    }

    /// Vertical or horizontal slider? The default is horizontal.
    #[inline]
    pub fn orientation(mut self, orientation: SliderOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Make this a vertical slider.
    #[inline]
    pub fn vertical(mut self) -> Self {
        self.orientation = SliderOrientation::Vertical;
        self
    }

    /// Make this a logarithmic slider.
    /// This is great for when the slider spans a huge range,
    /// e.g. from one to a million.
    /// The default is OFF.
    #[inline]
    pub fn logarithmic(mut self, logarithmic: bool) -> Self {
        self.spec.logarithmic = logarithmic;
        self
    }

    /// For logarithmic sliders that includes zero:
    /// what is the smallest positive value you want to be able to select?
    /// The default is `1` for integer sliders and `1e-6` for real sliders.
    #[inline]
    pub fn smallest_positive(mut self, smallest_positive: f64) -> Self {
        self.spec.smallest_positive = smallest_positive;
        self
    }

    /// For logarithmic sliders, the largest positive value we are interested in
    /// before the slider switches to `INFINITY`, if that is the higher end.
    /// Default: INFINITY.
    #[inline]
    pub fn largest_finite(mut self, largest_finite: f64) -> Self {
        self.spec.largest_finite = largest_finite;
        self
    }

    /// Controls when the values will be clamped to the range.
    ///
    /// ### With `.clamping(SliderClamping::Always)` (default)
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// let mut my_value: f32 = 1337.0;
    /// ui.add(egui::Slider::new(&mut my_value, 0.0..=1.0));
    /// assert!(0.0 <= my_value && my_value <= 1.0, "Existing value should be clamped");
    /// # });
    /// ```
    ///
    /// ### With `.clamping(SliderClamping::Edits)`
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// let mut my_value: f32 = 1337.0;
    /// let response = ui.add(
    ///     egui::Slider::new(&mut my_value, 0.0..=1.0)
    ///         .clamping(egui::SliderClamping::Edits)
    /// );
    /// if response.dragged() {
    ///     // The user edited the value, so it should now be clamped to the range
    ///     assert!(0.0 <= my_value && my_value <= 1.0);
    /// }
    /// # });
    /// ```
    ///
    /// ### With `.clamping(SliderClamping::Never)`
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// let mut my_value: f32 = 1337.0;
    /// let response = ui.add(
    ///     egui::Slider::new(&mut my_value, 0.0..=1.0)
    ///         .clamping(egui::SliderClamping::Never)
    /// );
    /// // The user could have set the value to anything
    /// # });
    /// ```
    #[inline]
    pub fn clamping(mut self, clamping: SliderClamping) -> Self {
        self.clamping = clamping;
        self
    }

    #[inline]
    #[deprecated = "Use `slider.clamping(…) instead"]
    pub fn clamp_to_range(self, clamp_to_range: bool) -> Self {
        self.clamping(if clamp_to_range {
            SliderClamping::Always
        } else {
            SliderClamping::Never
        })
    }

    /// Turn smart aim on/off. Default is ON.
    /// There is almost no point in turning this off.
    #[inline]
    pub fn smart_aim(mut self, smart_aim: bool) -> Self {
        self.smart_aim = smart_aim;
        self
    }

    /// Sets the minimal change of the value.
    ///
    /// Value `0.0` effectively disables the feature. If the new value is out of range
    /// and `clamp_to_range` is enabled, you would not have the ability to change the value.
    ///
    /// Default: `0.0` (disabled).
    #[inline]
    pub fn step_by(mut self, step: f64) -> Self {
        self.step = if step != 0.0 { Some(step) } else { None };
        self
    }

    /// When dragging the value, how fast does it move?
    ///
    /// Unit: values per point (logical pixel).
    /// See also [`DragValue::speed`].
    ///
    /// By default this is the same speed as when dragging the slider,
    /// but you can change it here to for instance have a much finer control
    /// by dragging the slider value rather than the slider itself.
    #[inline]
    pub fn drag_value_speed(mut self, drag_value_speed: f64) -> Self {
        self.drag_value_speed = Some(drag_value_speed);
        self
    }

    // TODO(emilk): we should also have a "min precision".
    /// Set a minimum number of decimals to display.
    ///
    /// Normally you don't need to pick a precision, as the slider will intelligently pick a precision for you.
    /// Regardless of precision the slider will use "smart aim" to help the user select nice, round values.
    #[inline]
    pub fn min_decimals(mut self, min_decimals: usize) -> Self {
        self.min_decimals = min_decimals;
        self
    }

    // TODO(emilk): we should also have a "max precision".
    /// Set a maximum number of decimals to display.
    ///
    /// Values will also be rounded to this number of decimals.
    /// Normally you don't need to pick a precision, as the slider will intelligently pick a precision for you.
    /// Regardless of precision the slider will use "smart aim" to help the user select nice, round values.
    #[inline]
    pub fn max_decimals(mut self, max_decimals: usize) -> Self {
        self.max_decimals = Some(max_decimals);
        self
    }

    #[inline]
    pub fn max_decimals_opt(mut self, max_decimals: Option<usize>) -> Self {
        self.max_decimals = max_decimals;
        self
    }

    /// Set an exact number of decimals to display.
    ///
    /// Values will also be rounded to this number of decimals.
    /// Normally you don't need to pick a precision, as the slider will intelligently pick a precision for you.
    /// Regardless of precision the slider will use "smart aim" to help the user select nice, round values.
    #[inline]
    pub fn fixed_decimals(mut self, num_decimals: usize) -> Self {
        self.min_decimals = num_decimals;
        self.max_decimals = Some(num_decimals);
        self
    }

    /// Display trailing color behind the slider's circle. Default is OFF.
    ///
    /// This setting can be enabled globally for all sliders with [`crate::Visuals::slider_trailing_fill`].
    /// Toggling it here will override the above setting ONLY for this individual slider.
    ///
    /// The fill color will be taken from `selection.bg_fill` in your [`crate::Visuals`], the same as a [`crate::ProgressBar`].
    #[inline]
    pub fn trailing_fill(mut self, trailing_fill: bool) -> Self {
        self.trailing_fill = Some(trailing_fill);
        self
    }

    /// Change the shape of the slider handle
    ///
    /// This setting can be enabled globally for all sliders with [`crate::Visuals::handle_shape`].
    /// Changing it here will override the above setting ONLY for this individual slider.
    #[inline]
    pub fn handle_shape(mut self, handle_shape: HandleShape) -> Self {
        self.handle_shape = Some(handle_shape);
        self
    }

    /// Set custom formatter defining how numbers are converted into text.
    ///
    /// A custom formatter takes a `f64` for the numeric value and a `RangeInclusive<usize>` representing
    /// the decimal range i.e. minimum and maximum number of decimal places shown.
    ///
    /// The default formatter is [`crate::Style::number_formatter`].
    ///
    /// See also: [`Slider::custom_parser`]
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::Slider::new(&mut my_i32, 0..=((60 * 60 * 24) - 1))
    ///     .custom_formatter(|n, _| {
    ///         let n = n as i32;
    ///         let hours = n / (60 * 60);
    ///         let mins = (n / 60) % 60;
    ///         let secs = n % 60;
    ///         format!("{hours:02}:{mins:02}:{secs:02}")
    ///     })
    ///     .custom_parser(|s| {
    ///         let parts: Vec<&str> = s.split(':').collect();
    ///         if parts.len() == 3 {
    ///             parts[0].parse::<i32>().and_then(|h| {
    ///                 parts[1].parse::<i32>().and_then(|m| {
    ///                     parts[2].parse::<i32>().map(|s| {
    ///                         ((h * 60 * 60) + (m * 60) + s) as f64
    ///                     })
    ///                 })
    ///             })
    ///             .ok()
    ///         } else {
    ///             None
    ///         }
    ///     }));
    /// # });
    /// ```
    pub fn custom_formatter(
        mut self,
        formatter: impl 'a + Fn(f64, RangeInclusive<usize>) -> String,
    ) -> Self {
        self.custom_formatter = Some(Box::new(formatter));
        self
    }

    /// Set custom parser defining how the text input is parsed into a number.
    ///
    /// A custom parser takes an `&str` to parse into a number and returns `Some` if it was successfully parsed
    /// or `None` otherwise.
    ///
    /// See also: [`Slider::custom_formatter`]
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::Slider::new(&mut my_i32, 0..=((60 * 60 * 24) - 1))
    ///     .custom_formatter(|n, _| {
    ///         let n = n as i32;
    ///         let hours = n / (60 * 60);
    ///         let mins = (n / 60) % 60;
    ///         let secs = n % 60;
    ///         format!("{hours:02}:{mins:02}:{secs:02}")
    ///     })
    ///     .custom_parser(|s| {
    ///         let parts: Vec<&str> = s.split(':').collect();
    ///         if parts.len() == 3 {
    ///             parts[0].parse::<i32>().and_then(|h| {
    ///                 parts[1].parse::<i32>().and_then(|m| {
    ///                     parts[2].parse::<i32>().map(|s| {
    ///                         ((h * 60 * 60) + (m * 60) + s) as f64
    ///                     })
    ///                 })
    ///             })
    ///             .ok()
    ///         } else {
    ///             None
    ///         }
    ///     }));
    /// # });
    /// ```
    #[inline]
    pub fn custom_parser(mut self, parser: impl 'a + Fn(&str) -> Option<f64>) -> Self {
        self.custom_parser = Some(Box::new(parser));
        self
    }

    /// Set `custom_formatter` and `custom_parser` to display and parse numbers as binary integers. Floating point
    /// numbers are *not* supported.
    ///
    /// `min_width` specifies the minimum number of displayed digits; if the number is shorter than this, it will be
    /// prefixed with additional 0s to match `min_width`.
    ///
    /// If `twos_complement` is true, negative values will be displayed as the 2's complement representation. Otherwise
    /// they will be prefixed with a '-' sign.
    ///
    /// # Panics
    ///
    /// Panics if `min_width` is 0.
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::Slider::new(&mut my_i32, -100..=100).binary(64, false));
    /// # });
    /// ```
    pub fn binary(self, min_width: usize, twos_complement: bool) -> Self {
        assert!(
            min_width > 0,
            "Slider::binary: `min_width` must be greater than 0"
        );
        if twos_complement {
            self.custom_formatter(move |n, _| format!("{:0>min_width$b}", n as i64))
        } else {
            self.custom_formatter(move |n, _| {
                let sign = if n < 0.0 { MINUS_CHAR_STR } else { "" };
                format!("{sign}{:0>min_width$b}", n.abs() as i64)
            })
        }
        .custom_parser(|s| i64::from_str_radix(s, 2).map(|n| n as f64).ok())
    }

    /// Set `custom_formatter` and `custom_parser` to display and parse numbers as octal integers. Floating point
    /// numbers are *not* supported.
    ///
    /// `min_width` specifies the minimum number of displayed digits; if the number is shorter than this, it will be
    /// prefixed with additional 0s to match `min_width`.
    ///
    /// If `twos_complement` is true, negative values will be displayed as the 2's complement representation. Otherwise
    /// they will be prefixed with a '-' sign.
    ///
    /// # Panics
    ///
    /// Panics if `min_width` is 0.
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::Slider::new(&mut my_i32, -100..=100).octal(22, false));
    /// # });
    /// ```
    pub fn octal(self, min_width: usize, twos_complement: bool) -> Self {
        assert!(
            min_width > 0,
            "Slider::octal: `min_width` must be greater than 0"
        );
        if twos_complement {
            self.custom_formatter(move |n, _| format!("{:0>min_width$o}", n as i64))
        } else {
            self.custom_formatter(move |n, _| {
                let sign = if n < 0.0 { MINUS_CHAR_STR } else { "" };
                format!("{sign}{:0>min_width$o}", n.abs() as i64)
            })
        }
        .custom_parser(|s| i64::from_str_radix(s, 8).map(|n| n as f64).ok())
    }

    /// Set `custom_formatter` and `custom_parser` to display and parse numbers as hexadecimal integers. Floating point
    /// numbers are *not* supported.
    ///
    /// `min_width` specifies the minimum number of displayed digits; if the number is shorter than this, it will be
    /// prefixed with additional 0s to match `min_width`.
    ///
    /// If `twos_complement` is true, negative values will be displayed as the 2's complement representation. Otherwise
    /// they will be prefixed with a '-' sign.
    ///
    /// # Panics
    ///
    /// Panics if `min_width` is 0.
    ///
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// # let mut my_i32: i32 = 0;
    /// ui.add(egui::Slider::new(&mut my_i32, -100..=100).hexadecimal(16, false, true));
    /// # });
    /// ```
    pub fn hexadecimal(self, min_width: usize, twos_complement: bool, upper: bool) -> Self {
        assert!(
            min_width > 0,
            "Slider::hexadecimal: `min_width` must be greater than 0"
        );
        match (twos_complement, upper) {
            (true, true) => {
                self.custom_formatter(move |n, _| format!("{:0>min_width$X}", n as i64))
            }
            (true, false) => {
                self.custom_formatter(move |n, _| format!("{:0>min_width$x}", n as i64))
            }
            (false, true) => self.custom_formatter(move |n, _| {
                let sign = if n < 0.0 { MINUS_CHAR_STR } else { "" };
                format!("{sign}{:0>min_width$X}", n.abs() as i64)
            }),
            (false, false) => self.custom_formatter(move |n, _| {
                let sign = if n < 0.0 { MINUS_CHAR_STR } else { "" };
                format!("{sign}{:0>min_width$x}", n.abs() as i64)
            }),
        }
        .custom_parser(|s| i64::from_str_radix(s, 16).map(|n| n as f64).ok())
    }

    /// Helper: equivalent to `self.precision(0).smallest_positive(1.0)`.
    /// If you use one of the integer constructors (e.g. `Slider::i32`) this is called for you,
    /// but if you want to have a slider for picking integer values in an `Slider::f64`, use this.
    pub fn integer(self) -> Self {
        self.fixed_decimals(0).smallest_positive(1.0).step_by(1.0)
    }

    fn get_value(&mut self) -> f64 {
        let value = get(&mut self.get_set_value);
        if self.clamping == SliderClamping::Always {
            clamp_value_to_range(value, self.range.clone())
        } else {
            value
        }
    }

    fn set_value(&mut self, mut value: f64) {
        if self.clamping != SliderClamping::Never {
            value = clamp_value_to_range(value, self.range.clone());
        }

        if let Some(step) = self.step {
            let start = *self.range.start();
            value = start + ((value - start) / step).round() * step;
        }
        if let Some(max_decimals) = self.max_decimals {
            value = emath::round_to_decimals(value, max_decimals);
        }
        set(&mut self.get_set_value, value);
    }

    fn range(&self) -> RangeInclusive<f64> {
        self.range.clone()
    }

    /// For instance, `position` is the mouse position and `position_range` is the physical location of the slider on the screen.
    fn value_from_position(&self, position: f32, position_range: Rangef) -> f64 {
        let normalized = remap_clamp(position, position_range, 0.0..=1.0) as f64;
        value_from_normalized(normalized, self.range(), &self.spec)
    }

    fn position_from_value(&self, value: f64, position_range: Rangef) -> f32 {
        let normalized = normalized_from_value(value, self.range(), &self.spec);
        lerp(position_range, normalized as f32)
    }

    /// Update the value on each key press when text-editing the value.
    ///
    /// Default: `true`.
    /// If `false`, the value will only be updated when user presses enter or deselects the value.
    #[inline]
    pub fn update_while_editing(mut self, update: bool) -> Self {
        self.update_while_editing = update;
        self
    }
}

impl Slider<'_> {
    /// Just the slider, no text
    fn allocate_slider_space(&self, ui: &mut Ui, thickness: f32) -> Response {
        let desired_size = match self.orientation {
            SliderOrientation::Horizontal => vec2(ui.spacing().slider_width, thickness),
            SliderOrientation::Vertical => vec2(thickness, ui.spacing().slider_width),
        };
        ui.allocate_response(desired_size, Sense::drag())
    }

    /// Just the slider, no text
    fn slider_ui(&mut self, ui: &Ui, response: &Response) {
        let rect = &response.rect;
        let handle_shape = self
            .handle_shape
            .unwrap_or_else(|| ui.style().visuals.handle_shape);
        let position_range = self.position_range(rect, &handle_shape);

        if let Some(pointer_position_2d) = response.interact_pointer_pos() {
            let position = self.pointer_position(pointer_position_2d);
            let new_value = if self.smart_aim {
                let aim_radius = ui.input(|i| i.aim_radius());
                emath::smart_aim::best_in_range_f64(
                    self.value_from_position(position - aim_radius, position_range),
                    self.value_from_position(position + aim_radius, position_range),
                )
            } else {
                self.value_from_position(position, position_range)
            };
            self.set_value(new_value);
        }

        let mut decrement = 0usize;
        let mut increment = 0usize;

        if response.has_focus() {
            ui.ctx().memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    EventFilter {
                        // pressing arrows in the orientation of the
                        // slider should not move focus to next widget
                        horizontal_arrows: matches!(
                            self.orientation,
                            SliderOrientation::Horizontal
                        ),
                        vertical_arrows: matches!(self.orientation, SliderOrientation::Vertical),
                        ..Default::default()
                    },
                );
            });

            let (dec_key, inc_key) = match self.orientation {
                SliderOrientation::Horizontal => (Key::ArrowLeft, Key::ArrowRight),
                // Note that this is for moving the slider position,
                // so up = decrement y coordinate:
                SliderOrientation::Vertical => (Key::ArrowUp, Key::ArrowDown),
            };

            ui.input(|input| {
                decrement += input.num_presses(dec_key);
                increment += input.num_presses(inc_key);
            });
        }

        #[cfg(feature = "accesskit")]
        {
            use accesskit::Action;
            ui.input(|input| {
                decrement += input.num_accesskit_action_requests(response.id, Action::Decrement);
                increment += input.num_accesskit_action_requests(response.id, Action::Increment);
            });
        }

        let kb_step = increment as f32 - decrement as f32;

        if kb_step != 0.0 {
            let ui_point_per_step = 1.0; // move this many ui points for each kb_step
            let prev_value = self.get_value();
            let prev_position = self.position_from_value(prev_value, position_range);
            let new_position = prev_position + ui_point_per_step * kb_step;
            let mut new_value = match self.step {
                Some(step) => prev_value + (kb_step as f64 * step),
                None if self.smart_aim => {
                    let aim_radius = 0.49 * ui_point_per_step; // Chosen so we don't include `prev_value` in the search.
                    emath::smart_aim::best_in_range_f64(
                        self.value_from_position(new_position - aim_radius, position_range),
                        self.value_from_position(new_position + aim_radius, position_range),
                    )
                }
                _ => self.value_from_position(new_position, position_range),
            };
            if let Some(max_decimals) = self.max_decimals {
                // self.set_value rounds, so ensure we reach at the least the next breakpoint
                // note: we give it a little bit of leeway due to floating point errors. (0.1 isn't representable in binary)
                // 'set_value' will round it to the nearest value.
                let min_increment = 1.0 / (10.0_f64.powi(max_decimals as i32));
                new_value = if new_value > prev_value {
                    f64::max(new_value, prev_value + min_increment * 1.001)
                } else if new_value < prev_value {
                    f64::min(new_value, prev_value - min_increment * 1.001)
                } else {
                    new_value
                };
            }
            self.set_value(new_value);
        }

        #[cfg(feature = "accesskit")]
        {
            use accesskit::{Action, ActionData};
            ui.input(|input| {
                for request in input.accesskit_action_requests(response.id, Action::SetValue) {
                    if let Some(ActionData::NumericValue(new_value)) = request.data {
                        self.set_value(new_value);
                    }
                }
            });
        }

        // Paint it:
        if ui.is_rect_visible(response.rect) {
            let value = self.get_value();

            let visuals = ui.style().interact(response);
            let widget_visuals = &ui.visuals().widgets;
            let spacing = &ui.style().spacing;

            let rail_radius = (spacing.slider_rail_height / 2.0).at_least(0.0);
            let rail_rect = self.rail_rect(rect, rail_radius);
            let corner_radius = widget_visuals.inactive.corner_radius;

            ui.painter()
                .rect_filled(rail_rect, corner_radius, widget_visuals.inactive.bg_fill);

            let position_1d = self.position_from_value(value, position_range);
            let center = self.marker_center(position_1d, &rail_rect);

            // Decide if we should add trailing fill.
            let trailing_fill = self
                .trailing_fill
                .unwrap_or_else(|| ui.visuals().slider_trailing_fill);

            // Paint trailing fill.
            if trailing_fill {
                let mut trailing_rail_rect = rail_rect;

                // The trailing rect has to be drawn differently depending on the orientation.
                match self.orientation {
                    SliderOrientation::Horizontal => {
                        trailing_rail_rect.max.x = center.x + corner_radius.nw as f32;
                    }
                    SliderOrientation::Vertical => {
                        trailing_rail_rect.min.y = center.y - corner_radius.se as f32;
                    }
                };

                ui.painter().rect_filled(
                    trailing_rail_rect,
                    corner_radius,
                    ui.visuals().selection.bg_fill,
                );
            }

            let radius = self.handle_radius(rect);

            let handle_shape = self
                .handle_shape
                .unwrap_or_else(|| ui.style().visuals.handle_shape);
            match handle_shape {
                style::HandleShape::Circle => {
                    ui.painter().add(epaint::CircleShape {
                        center,
                        radius: radius + visuals.expansion,
                        fill: visuals.bg_fill,
                        stroke: visuals.fg_stroke,
                    });
                }
                style::HandleShape::Rect { aspect_ratio } => {
                    let v = match self.orientation {
                        SliderOrientation::Horizontal => Vec2::new(radius * aspect_ratio, radius),
                        SliderOrientation::Vertical => Vec2::new(radius, radius * aspect_ratio),
                    };
                    let v = v + Vec2::splat(visuals.expansion);
                    let rect = Rect::from_center_size(center, 2.0 * v);
                    ui.painter().rect(
                        rect,
                        visuals.corner_radius,
                        visuals.bg_fill,
                        visuals.fg_stroke,
                        epaint::StrokeKind::Inside,
                    );
                }
            }
        }
    }

    fn marker_center(&self, position_1d: f32, rail_rect: &Rect) -> Pos2 {
        match self.orientation {
            SliderOrientation::Horizontal => pos2(position_1d, rail_rect.center().y),
            SliderOrientation::Vertical => pos2(rail_rect.center().x, position_1d),
        }
    }

    fn pointer_position(&self, pointer_position_2d: Pos2) -> f32 {
        match self.orientation {
            SliderOrientation::Horizontal => pointer_position_2d.x,
            SliderOrientation::Vertical => pointer_position_2d.y,
        }
    }

    fn position_range(&self, rect: &Rect, handle_shape: &style::HandleShape) -> Rangef {
        let handle_radius = self.handle_radius(rect);
        let handle_radius = match handle_shape {
            style::HandleShape::Circle => handle_radius,
            style::HandleShape::Rect { aspect_ratio } => handle_radius * aspect_ratio,
        };
        match self.orientation {
            SliderOrientation::Horizontal => rect.x_range().shrink(handle_radius),
            // The vertical case has to be flipped because the largest slider value maps to the
            // lowest y value (which is at the top)
            SliderOrientation::Vertical => rect.y_range().shrink(handle_radius).flip(),
        }
    }

    fn rail_rect(&self, rect: &Rect, radius: f32) -> Rect {
        match self.orientation {
            SliderOrientation::Horizontal => Rect::from_min_max(
                pos2(rect.left(), rect.center().y - radius),
                pos2(rect.right(), rect.center().y + radius),
            ),
            SliderOrientation::Vertical => Rect::from_min_max(
                pos2(rect.center().x - radius, rect.top()),
                pos2(rect.center().x + radius, rect.bottom()),
            ),
        }
    }

    fn handle_radius(&self, rect: &Rect) -> f32 {
        let limit = match self.orientation {
            SliderOrientation::Horizontal => rect.height(),
            SliderOrientation::Vertical => rect.width(),
        };
        limit / 2.5
    }

    fn value_ui(&mut self, ui: &mut Ui, position_range: Rangef) -> Response {
        // If [`DragValue`] is controlled from the keyboard and `step` is defined, set speed to `step`
        let change = ui.input(|input| {
            input.num_presses(Key::ArrowUp) as i32 + input.num_presses(Key::ArrowRight) as i32
                - input.num_presses(Key::ArrowDown) as i32
                - input.num_presses(Key::ArrowLeft) as i32
        });

        let any_change = change != 0;
        let speed = if let (Some(step), true) = (self.step, any_change) {
            // If [`DragValue`] is controlled from the keyboard and `step` is defined, set speed to `step`
            step
        } else {
            self.drag_value_speed
                .unwrap_or_else(|| self.current_gradient(position_range))
        };

        let mut value = self.get_value();
        let response = ui.add({
            let mut dv = DragValue::new(&mut value)
                .speed(speed)
                .min_decimals(self.min_decimals)
                .max_decimals_opt(self.max_decimals)
                .suffix(self.suffix.clone())
                .prefix(self.prefix.clone())
                .update_while_editing(self.update_while_editing);

            match self.clamping {
                SliderClamping::Never => {}
                SliderClamping::Edits => {
                    dv = dv.range(self.range.clone()).clamp_existing_to_range(false);
                }
                SliderClamping::Always => {
                    dv = dv.range(self.range.clone()).clamp_existing_to_range(true);
                }
            }

            if let Some(fmt) = &self.custom_formatter {
                dv = dv.custom_formatter(fmt);
            };
            if let Some(parser) = &self.custom_parser {
                dv = dv.custom_parser(parser);
            }
            dv
        });
        if value != self.get_value() {
            self.set_value(value);
        }
        response
    }

    /// delta(value) / delta(points)
    fn current_gradient(&mut self, position_range: Rangef) -> f64 {
        // TODO(emilk): handle clamping
        let value = self.get_value();
        let value_from_pos = |position: f32| self.value_from_position(position, position_range);
        let pos_from_value = |value: f64| self.position_from_value(value, position_range);
        let left_value = value_from_pos(pos_from_value(value) - 0.5);
        let right_value = value_from_pos(pos_from_value(value) + 0.5);
        right_value - left_value
    }

    fn add_contents(&mut self, ui: &mut Ui) -> Response {
        let old_value = self.get_value();

        if self.clamping == SliderClamping::Always {
            self.set_value(old_value);
        }

        let thickness = ui
            .text_style_height(&TextStyle::Body)
            .at_least(ui.spacing().interact_size.y);
        let mut response = self.allocate_slider_space(ui, thickness);
        self.slider_ui(ui, &response);

        let value = self.get_value();
        if value != old_value {
            response.mark_changed();
        }
        response.widget_info(|| WidgetInfo::slider(ui.is_enabled(), value, self.text.text()));

        #[cfg(feature = "accesskit")]
        ui.ctx().accesskit_node_builder(response.id, |builder| {
            use accesskit::Action;
            builder.set_min_numeric_value(*self.range.start());
            builder.set_max_numeric_value(*self.range.end());
            if let Some(step) = self.step {
                builder.set_numeric_value_step(step);
            }
            builder.add_action(Action::SetValue);

            let clamp_range = if self.clamping == SliderClamping::Never {
                f64::NEG_INFINITY..=f64::INFINITY
            } else {
                self.range()
            };
            if value < *clamp_range.end() {
                builder.add_action(Action::Increment);
            }
            if value > *clamp_range.start() {
                builder.add_action(Action::Decrement);
            }
        });

        let slider_response = response.clone();

        let value_response = if self.show_value {
            let handle_shape = self
                .handle_shape
                .unwrap_or_else(|| ui.style().visuals.handle_shape);
            let position_range = self.position_range(&response.rect, &handle_shape);
            let value_response = self.value_ui(ui, position_range);
            if value_response.gained_focus()
                || value_response.has_focus()
                || value_response.lost_focus()
            {
                // Use the [`DragValue`] id as the id of the whole widget,
                // so that the focus events work as expected.
                response = value_response.union(response);
            } else {
                // Use the slider id as the id for the whole widget
                response = response.union(value_response.clone());
            }
            Some(value_response)
        } else {
            None
        };

        if !self.text.is_empty() {
            let label_response =
                ui.add(Label::new(self.text.clone()).wrap_mode(TextWrapMode::Extend));
            // The slider already has an accessibility label via widget info,
            // but sometimes it's useful for a screen reader to know
            // that a piece of text is a label for another widget,
            // e.g. so the text itself can be excluded from navigation.
            slider_response.labelled_by(label_response.id);
            if let Some(value_response) = value_response {
                value_response.labelled_by(label_response.id);
            }
        }

        response
    }
}

impl Widget for Slider<'_> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let inner_response = match self.orientation {
            SliderOrientation::Horizontal => ui.horizontal(|ui| self.add_contents(ui)),
            SliderOrientation::Vertical => ui.vertical(|ui| self.add_contents(ui)),
        };

        inner_response.inner | inner_response.response
    }
}

// ----------------------------------------------------------------------------
// Helpers for converting slider range to/from normalized [0-1] range.
// Always clamps.
// Logarithmic sliders are allowed to include zero and infinity,
// even though mathematically it doesn't make sense.

const INFINITY: f64 = f64::INFINITY;

/// When the user asks for an infinitely large range (e.g. logarithmic from zero),
/// give a scale that this many orders of magnitude in size.
const INF_RANGE_MAGNITUDE: f64 = 10.0;

fn value_from_normalized(normalized: f64, range: RangeInclusive<f64>, spec: &SliderSpec) -> f64 {
    let (min, max) = (*range.start(), *range.end());

    if min.is_nan() || max.is_nan() {
        f64::NAN
    } else if min == max {
        min
    } else if min > max {
        value_from_normalized(1.0 - normalized, max..=min, spec)
    } else if normalized <= 0.0 {
        min
    } else if normalized >= 1.0 {
        max
    } else if spec.logarithmic {
        if max <= 0.0 {
            // non-positive range
            -value_from_normalized(normalized, -min..=-max, spec)
        } else if 0.0 <= min {
            let (min_log, max_log) = range_log10(min, max, spec);
            let log = lerp(min_log..=max_log, normalized);
            10.0_f64.powf(log)
        } else {
            assert!(
                min < 0.0 && 0.0 < max,
                "min should be negative and max positive, but got min={min} and max={max}"
            );
            let zero_cutoff = logarithmic_zero_cutoff(min, max);
            if normalized < zero_cutoff {
                // negative
                value_from_normalized(
                    remap(normalized, 0.0..=zero_cutoff, 0.0..=1.0),
                    min..=0.0,
                    spec,
                )
            } else {
                // positive
                value_from_normalized(
                    remap(normalized, zero_cutoff..=1.0, 0.0..=1.0),
                    0.0..=max,
                    spec,
                )
            }
        }
    } else {
        debug_assert!(
            min.is_finite() && max.is_finite(),
            "You should use a logarithmic range"
        );
        lerp(range, normalized.clamp(0.0, 1.0))
    }
}

fn normalized_from_value(value: f64, range: RangeInclusive<f64>, spec: &SliderSpec) -> f64 {
    let (min, max) = (*range.start(), *range.end());

    if min.is_nan() || max.is_nan() {
        f64::NAN
    } else if min == max {
        0.5 // empty range, show center of slider
    } else if min > max {
        1.0 - normalized_from_value(value, max..=min, spec)
    } else if value <= min {
        0.0
    } else if value >= max {
        1.0
    } else if spec.logarithmic {
        if max <= 0.0 {
            // non-positive range
            normalized_from_value(-value, -min..=-max, spec)
        } else if 0.0 <= min {
            let (min_log, max_log) = range_log10(min, max, spec);
            let value_log = value.log10();
            remap_clamp(value_log, min_log..=max_log, 0.0..=1.0)
        } else {
            assert!(
                min < 0.0 && 0.0 < max,
                "min should be negative and max positive, but got min={min} and max={max}"
            );
            let zero_cutoff = logarithmic_zero_cutoff(min, max);
            if value < 0.0 {
                // negative
                remap(
                    normalized_from_value(value, min..=0.0, spec),
                    0.0..=1.0,
                    0.0..=zero_cutoff,
                )
            } else {
                // positive side
                remap(
                    normalized_from_value(value, 0.0..=max, spec),
                    0.0..=1.0,
                    zero_cutoff..=1.0,
                )
            }
        }
    } else {
        debug_assert!(
            min.is_finite() && max.is_finite(),
            "You should use a logarithmic range"
        );
        remap_clamp(value, range, 0.0..=1.0)
    }
}

fn range_log10(min: f64, max: f64, spec: &SliderSpec) -> (f64, f64) {
    assert!(spec.logarithmic, "spec must be logarithmic");
    assert!(
        min <= max,
        "min must be less than or equal to max, but was min={min} and max={max}"
    );

    if min == 0.0 && max == INFINITY {
        (spec.smallest_positive.log10(), INF_RANGE_MAGNITUDE)
    } else if min == 0.0 {
        if spec.smallest_positive < max {
            (spec.smallest_positive.log10(), max.log10())
        } else {
            (max.log10() - INF_RANGE_MAGNITUDE, max.log10())
        }
    } else if max == INFINITY {
        if min < spec.largest_finite {
            (min.log10(), spec.largest_finite.log10())
        } else {
            (min.log10(), min.log10() + INF_RANGE_MAGNITUDE)
        }
    } else {
        (min.log10(), max.log10())
    }
}

/// where to put the zero cutoff for logarithmic sliders
/// that crosses zero ?
fn logarithmic_zero_cutoff(min: f64, max: f64) -> f64 {
    assert!(
        min < 0.0 && 0.0 < max,
        "min must be negative and max positive, but got min={min} and max={max}"
    );

    let min_magnitude = if min == -INFINITY {
        INF_RANGE_MAGNITUDE
    } else {
        min.abs().log10().abs()
    };
    let max_magnitude = if max == INFINITY {
        INF_RANGE_MAGNITUDE
    } else {
        max.log10().abs()
    };

    let cutoff = min_magnitude / (min_magnitude + max_magnitude);
    debug_assert!(
        0.0 <= cutoff && cutoff <= 1.0,
        "Bad cutoff {cutoff:?} for min {min:?} and max {max:?}"
    );
    cutoff
}
