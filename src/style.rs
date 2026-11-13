//! Visual styling types, RTS palette, and keyboard hotkey definitions.

/// RGBA color in linear-ish sRGB `[0.0..=1.0]` floats for GPU rendering.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(0.95, 0.97, 1.0);
    pub const BLACK: Self = Self::rgb(0.04, 0.05, 0.07);

    pub const HUD_BG: Self = Self::rgba(0.045, 0.065, 0.095, 0.90);
    pub const HUD_PANEL: Self = Self::rgba(0.075, 0.105, 0.150, 0.94);
    pub const HUD_BORDER: Self = Self::rgba(0.22, 0.36, 0.50, 0.85);
    pub const HUD_BORDER_HIGHLIGHT: Self = Self::rgba(0.38, 0.72, 0.95, 1.0);

    pub const ACCENT_CYAN: Self = Self::rgb(0.25, 0.82, 0.98);
    pub const ACCENT_GOLD: Self = Self::rgb(0.96, 0.76, 0.28);
    pub const MINERAL_BLUE: Self = Self::rgb(0.32, 0.78, 1.0);
    pub const GAS_GREEN: Self = Self::rgb(0.34, 0.90, 0.52);

    pub const HEALTH_GREEN: Self = Self::rgb(0.24, 0.86, 0.42);
    pub const HEALTH_YELLOW: Self = Self::rgb(0.95, 0.78, 0.22);
    pub const HEALTH_RED: Self = Self::rgb(0.92, 0.26, 0.24);
    pub const SHIELD_BLUE: Self = Self::rgb(0.26, 0.65, 0.98);
    pub const DANGER_RED: Self = Self::rgb(0.95, 0.30, 0.28);

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Creates a `Color` from a `0xRRGGBB` hex literal.
    pub const fn from_hex(hex: u32) -> Self {
        let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let b = (hex & 0xFF) as f32 / 255.0;
        Self { r, g, b, a: 1.0 }
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Visual box decoration for `Container` widgets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxDecoration {
    pub bg_color: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub corner_radius: f32,
}

impl Default for BoxDecoration {
    fn default() -> Self {
        Self {
            bg_color: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radius: 0.0,
        }
    }
}

impl BoxDecoration {
    pub const fn hud_panel() -> Self {
        Self {
            bg_color: Color::HUD_BG,
            border_color: Color::HUD_BORDER,
            border_width: 1.5,
            corner_radius: 6.0,
        }
    }
}

/// Typography settings for `Text` widgets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub color: Color,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            color: Color::WHITE,
        }
    }
}

impl TextStyle {
    pub const fn new(font_size: f32, color: Color) -> Self {
        Self { font_size, color }
    }
}

/// Styling for command card and HUD buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonStyle {
    pub normal_bg: Color,
    pub hover_bg: Color,
    pub pressed_bg: Color,
    pub selected_bg: Color,
    pub disabled_bg: Color,
    pub border_color: Color,
    pub selected_border_color: Color,
    pub hover_border_color: Color,
    pub text_color: Color,
    pub disabled_text_color: Color,
    pub corner_radius: f32,
    pub border_width: f32,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            normal_bg: Color::rgba(0.09, 0.13, 0.19, 0.95),
            hover_bg: Color::rgba(0.14, 0.22, 0.32, 0.98),
            pressed_bg: Color::rgba(0.06, 0.10, 0.15, 1.0),
            selected_bg: Color::rgba(0.14, 0.28, 0.38, 0.98),
            disabled_bg: Color::rgba(0.06, 0.07, 0.09, 0.72),
            border_color: Color::HUD_BORDER,
            selected_border_color: Color::ACCENT_GOLD,
            hover_border_color: Color::HUD_BORDER_HIGHLIGHT,
            text_color: Color::WHITE,
            disabled_text_color: Color::rgba(0.45, 0.50, 0.56, 0.8),
            corner_radius: 5.0,
            border_width: 1.5,
        }
    }
}

/// Logical keyboard hotkeys bound to RTS command buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Q,
    W,
    E,
    R,
    T,
    A,
    S,
    D,
    F,
    Z,
    X,
    C,
    V,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Escape,
    Space,
}

impl KeyCode {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Q => "Q",
            Self::W => "W",
            Self::E => "E",
            Self::R => "R",
            Self::T => "T",
            Self::A => "A",
            Self::S => "S",
            Self::D => "D",
            Self::F => "F",
            Self::Z => "Z",
            Self::X => "X",
            Self::C => "C",
            Self::V => "V",
            Self::Digit1 => "1",
            Self::Digit2 => "2",
            Self::Digit3 => "3",
            Self::Digit4 => "4",
            Self::Escape => "Esc",
            Self::Space => "Space",
        }
    }

    pub fn from_winit(key: winit::keyboard::KeyCode) -> Option<Self> {
        use winit::keyboard::KeyCode as WK;
        match key {
            WK::KeyQ => Some(Self::Q),
            WK::KeyW => Some(Self::W),
            WK::KeyE => Some(Self::E),
            WK::KeyR => Some(Self::R),
            WK::KeyT => Some(Self::T),
            WK::KeyA => Some(Self::A),
            WK::KeyS => Some(Self::S),
            WK::KeyD => Some(Self::D),
            WK::KeyF => Some(Self::F),
            WK::KeyZ => Some(Self::Z),
            WK::KeyX => Some(Self::X),
            WK::KeyC => Some(Self::C),
            WK::KeyV => Some(Self::V),
            WK::Digit1 => Some(Self::Digit1),
            WK::Digit2 => Some(Self::Digit2),
            WK::Digit3 => Some(Self::Digit3),
            WK::Digit4 => Some(Self::Digit4),
            WK::Escape => Some(Self::Escape),
            WK::Space => Some(Self::Space),
            _ => None,
        }
    }
}
