//! Interactive RTS HUD showcase built with TEA + Flutter-style constraint widgets.

use std::sync::Arc;

use crate::geom::{Alignment, EdgeInsets, Rect, Vec2};
use crate::style::{BoxDecoration, ButtonStyle, Color, KeyCode, TextStyle};
use crate::widget::{
    Application, ButtonWidget, CrossAxisAlignment, FlexChild, MainAxisAlignment, StackChild,
    TooltipSpec, Widget, WorldCanvas, WorldOrderMarker, WorldUnitOverlay,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitAction {
    MoveMode,
    Stop,
    HoldPosition,
    AttackMove,
    TriggerStimpack,
    NanoRepair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Scv,
    Marine,
    Medic,
    SiegeTank,
    CommandCenter,
    Barracks,
}

impl UnitKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scv => "SCV",
            Self::Marine => "Marine",
            Self::Medic => "Medic",
            Self::SiegeTank => "Siege Tank",
            Self::CommandCenter => "Command Center",
            Self::Barracks => "Barracks",
        }
    }

    pub const fn cost(self) -> (u32, u32, u32) {
        // (minerals, gas, supply)
        match self {
            Self::Scv => (50, 0, 1),
            Self::Marine => (50, 0, 1),
            Self::Medic => (50, 25, 1),
            Self::SiegeTank => (150, 125, 3),
            Self::CommandCenter => (400, 0, 0),
            Self::Barracks => (150, 0, 0),
        }
    }

    pub const fn train_duration(self) -> f32 {
        match self {
            Self::Scv => 4.5,
            Self::Marine => 4.0,
            Self::Medic => 5.5,
            Self::SiegeTank => 8.0,
            _ => 0.0,
        }
    }

    pub const fn max_hp(self) -> f32 {
        match self {
            Self::Scv => 60.0,
            Self::Marine => 55.0,
            Self::Medic => 75.0,
            Self::SiegeTank => 175.0,
            Self::CommandCenter => 1500.0,
            Self::Barracks => 1000.0,
        }
    }

    pub const fn max_energy(self) -> Option<f32> {
        match self {
            Self::Medic => Some(200.0),
            Self::CommandCenter => Some(200.0),
            _ => None,
        }
    }

    pub const fn radius(self) -> f32 {
        match self {
            Self::Scv => 13.0,
            Self::Marine => 13.0,
            Self::Medic => 14.0,
            Self::SiegeTank => 19.0,
            Self::CommandCenter => 34.0,
            Self::Barracks => 29.0,
        }
    }

    pub const fn color(self) -> Color {
        match self {
            Self::Scv => Color::from_hex(0xD99B38),
            Self::Marine => Color::from_hex(0x3B82F6),
            Self::Medic => Color::from_hex(0xE2E8F0),
            Self::SiegeTank => Color::from_hex(0x4B8B6B),
            Self::CommandCenter => Color::from_hex(0x2D5F9A),
            Self::Barracks => Color::from_hex(0x52647A),
        }
    }

    pub const fn is_building(self) -> bool {
        matches!(self, Self::CommandCenter | Self::Barracks)
    }

    pub const fn weapon_info(self) -> &'static str {
        match self {
            Self::Scv => "Fusion Cutter (5 dmg) | Armor: 0",
            Self::Marine => "C-14 Gauss Rifle (6 dmg) | Armor: 0",
            Self::Medic => "Caduceus Reactor (Heal 12/s) | Armor: 1",
            Self::SiegeTank => "90mm Cannon (30 dmg) | Armor: 1",
            Self::CommandCenter => "Planetary Hub | Armor: 1 | +11 Supply",
            Self::Barracks => "Infantry & Armor Bay | Armor: 1",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnitEntity {
    pub id: u32,
    pub kind: UnitKind,
    pub pos: Vec2,
    pub target: Option<Vec2>,
    pub hp: f32,
    pub max_hp: f32,
    pub energy: Option<(f32, f32)>,
    pub speed: f32,
    pub stim_timer: f32,
    pub stim_cooldown: f32,
    pub production_queue: Vec<UnitKind>,
    pub production_elapsed: f32,
}

impl UnitEntity {
    fn new(id: u32, kind: UnitKind, x: f32, y: f32, hp_ratio: f32) -> Self {
        let max_hp = kind.max_hp();
        let energy = kind.max_energy().map(|m| (m * 0.72, m));
        Self {
            id,
            kind,
            pos: Vec2::new(x, y),
            target: None,
            hp: max_hp * hp_ratio,
            max_hp,
            energy,
            speed: match kind {
                UnitKind::Marine => 115.0,
                UnitKind::Medic => 110.0,
                UnitKind::Scv => 105.0,
                UnitKind::SiegeTank => 82.0,
                _ => 0.0,
            },
            stim_timer: 0.0,
            stim_cooldown: 0.0,
            production_queue: Vec::new(),
            production_elapsed: 0.0,
        }
    }

    pub fn active_production_fraction(&self) -> Option<f32> {
        let first = self.production_queue.first()?;
        let dur = first.train_duration().max(0.1);
        Some((self.production_elapsed / dur).clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone)]
pub enum RtsMessage {
    Tick(f32),
    SelectUnit(Option<u32>, bool),
    BoxSelect(Rect, bool),
    IssueMoveOrder(Vec2),
    FocusUnitFromGroup(u32),
    ClearSelection,
    UnitCommand(UnitAction),
    TrainUnit(UnitKind),
    CancelProduction(usize),
    TogglePause,
}

#[derive(Debug, Clone)]
pub struct RtsDemoApp {
    pub minerals: u32,
    pub gas: u32,
    pub supply_cap: u32,
    pub elapsed_time: f32,
    pub paused: bool,
    pub hud_wants_pointer: bool,
    pub active_command: Option<UnitAction>,
    pub units: Vec<UnitEntity>,
    pub selected_ids: Vec<u32>,
    pub order_markers: Vec<WorldOrderMarker>,
    pub next_unit_id: u32,
    pub status_banner: String,
}

impl Default for RtsDemoApp {
    fn default() -> Self {
        Self::new()
    }
}

impl RtsDemoApp {
    pub fn new() -> Self {
        let mut barracks = UnitEntity::new(2, UnitKind::Barracks, 360.0, 250.0, 1.0);
        barracks.production_queue.push(UnitKind::Marine);
        barracks.production_elapsed = 1.6;

        let units = vec![
            UnitEntity::new(1, UnitKind::CommandCenter, 190.0, 240.0, 1.0),
            barracks,
            UnitEntity::new(3, UnitKind::Scv, 250.0, 325.0, 1.0),
            UnitEntity::new(4, UnitKind::Scv, 290.0, 340.0, 0.85),
            UnitEntity::new(5, UnitKind::Marine, 530.0, 270.0, 1.0),
            UnitEntity::new(6, UnitKind::Marine, 575.0, 295.0, 0.78),
            UnitEntity::new(7, UnitKind::Marine, 545.0, 340.0, 0.45),
            UnitEntity::new(8, UnitKind::Medic, 500.0, 315.0, 0.92),
            UnitEntity::new(9, UnitKind::SiegeTank, 650.0, 310.0, 0.96),
        ];

        Self {
            minerals: 450,
            gas: 225,
            supply_cap: 24,
            elapsed_time: 142.0,
            paused: false,
            hud_wants_pointer: false,
            active_command: None,
            units,
            selected_ids: vec![5, 6, 7, 8],
            order_markers: Vec::new(),
            next_unit_id: 10,
            status_banner: "Squad Alpha Ready".to_string(),
        }
    }

    pub fn supply_used(&self) -> u32 {
        self.units
            .iter()
            .filter(|u| !u.kind.is_building())
            .map(|u| u.kind.cost().2)
            .sum()
    }

    fn selected_units(&self) -> Vec<&UnitEntity> {
        self.units
            .iter()
            .filter(|u| self.selected_ids.contains(&u.id))
            .collect()
    }

    fn can_afford(&self, kind: UnitKind) -> bool {
        let (min, gas, sup) = kind.cost();
        self.minerals >= min
            && self.gas >= gas
            && self.supply_used() + sup <= self.supply_cap
    }
}

impl Application for RtsDemoApp {
    type Message = RtsMessage;

    fn update(&mut self, message: Self::Message) {
        match message {
            RtsMessage::Tick(dt) => {
                if self.paused {
                    return;
                }
                let dt = dt.clamp(0.0, 0.1);
                self.elapsed_time += dt;

                // Passive mineral/gas income from SCVs
                let scv_count = self
                    .units
                    .iter()
                    .filter(|u| u.kind == UnitKind::Scv)
                    .count() as f32;
                let prev_sec = (self.elapsed_time - dt).floor() as u32;
                let cur_sec = self.elapsed_time.floor() as u32;
                if cur_sec > prev_sec {
                    self.minerals += (scv_count * 2.0) as u32;
                    self.gas += 1;
                }

                // Fade order markers
                for marker in &mut self.order_markers {
                    marker.ttl -= dt * 1.4;
                }
                self.order_markers.retain(|m| m.ttl > 0.0);

                // Move units, update cooldowns & production
                let mut spawned = Vec::new();
                for unit in &mut self.units {
                    if unit.stim_timer > 0.0 {
                        unit.stim_timer = (unit.stim_timer - dt).max(0.0);
                    }
                    if unit.stim_cooldown > 0.0 {
                        unit.stim_cooldown = (unit.stim_cooldown - dt).max(0.0);
                    }
                    if let Some((e, max_e)) = &mut unit.energy {
                        *e = (*e + dt * 1.5).min(*max_e);
                    }

                    if let Some(target) = unit.target {
                        let delta = Vec2::new(target.x - unit.pos.x, target.y - unit.pos.y);
                        let dist = delta.length();
                        let speed_mult = if unit.stim_timer > 0.0 { 1.65 } else { 1.0 };
                        let step = unit.speed * speed_mult * dt;
                        if dist <= step || dist < 1.5 {
                            unit.pos = target;
                            unit.target = None;
                        } else {
                            let dir = delta.normalized();
                            unit.pos.x += dir.x * step;
                            unit.pos.y += dir.y * step;
                        }
                    }

                    if let Some(&kind) = unit.production_queue.first() {
                        unit.production_elapsed += dt;
                        if unit.production_elapsed >= kind.train_duration() {
                            unit.production_elapsed = 0.0;
                            unit.production_queue.remove(0);
                            let offset_idx = self.next_unit_id as f32;
                            let spawn_x = unit.pos.x + 65.0 + (offset_idx % 4.0) * 26.0;
                            let spawn_y = unit.pos.y + 42.0 + ((offset_idx / 4.0).floor() % 3.0) * 26.0;
                            spawned.push((kind, spawn_x, spawn_y));
                        }
                    }
                }

                for (kind, sx, sy) in spawned {
                    let id = self.next_unit_id;
                    self.next_unit_id += 1;
                    self.units.push(UnitEntity::new(id, kind, sx, sy, 1.0));
                    self.status_banner = format!("{} deployed", kind.name());
                }
            }

            RtsMessage::SelectUnit(opt_id, additive) => {
                if let Some(id) = opt_id {
                    if additive {
                        if !self.selected_ids.contains(&id) {
                            self.selected_ids.push(id);
                        }
                    } else {
                        self.selected_ids = vec![id];
                    }
                    if let Some(u) = self.units.iter().find(|u| u.id == id) {
                        self.status_banner = format!("Selected {}", u.kind.name());
                    }
                } else if !additive {
                    self.selected_ids.clear();
                    self.status_banner = "Selection cleared".to_string();
                }
                self.active_command = None;
            }

            RtsMessage::BoxSelect(rect, additive) => {
                // Prioritize mobile units in box selection (RTS convention) unless only buildings exist.
                let mut mobile_hits: Vec<u32> = self
                    .units
                    .iter()
                    .filter(|u| !u.kind.is_building() && rect.contains(u.pos))
                    .map(|u| u.id)
                    .collect();

                if mobile_hits.is_empty() {
                    mobile_hits = self
                        .units
                        .iter()
                        .filter(|u| rect.contains(u.pos))
                        .map(|u| u.id)
                        .collect();
                }

                if additive {
                    for id in mobile_hits {
                        if !self.selected_ids.contains(&id) {
                            self.selected_ids.push(id);
                        }
                    }
                } else {
                    self.selected_ids = mobile_hits;
                }
                self.active_command = None;
                self.status_banner = format!("Selected {} unit(s)", self.selected_ids.len());
            }

            RtsMessage::IssueMoveOrder(target) => {
                let selected_count = self
                    .units
                    .iter()
                    .filter(|u| self.selected_ids.contains(&u.id) && !u.kind.is_building())
                    .count();
                if selected_count == 0 {
                    return;
                }

                self.order_markers.push(WorldOrderMarker {
                    pos: target,
                    ttl: 1.0,
                });

                // Spread units slightly around target in a clean formation.
                let mut slot = 0usize;
                for unit in &mut self.units {
                    if self.selected_ids.contains(&unit.id) && !unit.kind.is_building() {
                        let angle = (slot as f32) * 1.45;
                        let ring = if slot == 0 { 0.0 } else { 24.0 + (slot as f32) * 4.5 };
                        unit.target = Some(Vec2::new(
                            target.x + angle.cos() * ring,
                            target.y + angle.sin() * ring,
                        ));
                        slot += 1;
                    }
                }
                self.active_command = None;
                self.status_banner = format!("Moving {} unit(s)", selected_count);
            }

            RtsMessage::FocusUnitFromGroup(id) => {
                if self.units.iter().any(|u| u.id == id) {
                    self.selected_ids = vec![id];
                    self.status_banner = "Focused unit".to_string();
                }
            }

            RtsMessage::ClearSelection => {
                self.selected_ids.clear();
                self.active_command = None;
                self.status_banner = "Selection cleared".to_string();
            }

            RtsMessage::UnitCommand(action) => {
                match action {
                    UnitAction::Stop => {
                        for u in &mut self.units {
                            if self.selected_ids.contains(&u.id) {
                                u.target = None;
                            }
                        }
                        self.active_command = None;
                        self.status_banner = "Stop order issued".to_string();
                    }
                    UnitAction::HoldPosition => {
                        for u in &mut self.units {
                            if self.selected_ids.contains(&u.id) {
                                u.target = None;
                            }
                        }
                        self.active_command = Some(UnitAction::HoldPosition);
                        self.status_banner = "Holding position".to_string();
                    }
                    UnitAction::TriggerStimpack => {
                        let mut stimulated = 0;
                        for u in &mut self.units {
                            if self.selected_ids.contains(&u.id)
                                && u.kind == UnitKind::Marine
                                && u.stim_cooldown <= 0.0
                                && u.hp > 10.0
                            {
                                u.hp -= 10.0;
                                u.stim_timer = 5.0;
                                u.stim_cooldown = 8.0;
                                stimulated += 1;
                            }
                        }
                        if stimulated > 0 {
                            self.status_banner = format!("Stimpack active ({stimulated} Marine(s))");
                        }
                    }
                    UnitAction::NanoRepair => {
                        let mut healed = 0;
                        for u in &mut self.units {
                            if self.selected_ids.contains(&u.id) && !u.kind.is_building() {
                                u.hp = (u.hp + 20.0).min(u.max_hp);
                                healed += 1;
                            }
                        }
                        self.status_banner = format!("Nano-Repair applied to {healed} unit(s)");
                    }
                    UnitAction::MoveMode | UnitAction::AttackMove => {
                        self.active_command = Some(action);
                        self.status_banner = if action == UnitAction::AttackMove {
                            "Attack-Move armed (right-click ground)".to_string()
                        } else {
                            "Move armed (right-click ground)".to_string()
                        };
                    }
                }
            }

            RtsMessage::TrainUnit(kind) => {
                if !self.can_afford(kind) {
                    self.status_banner = "Insufficient resources or supply!".to_string();
                    return;
                }
                let producer_kind = match kind {
                    UnitKind::Scv => UnitKind::CommandCenter,
                    UnitKind::Marine | UnitKind::Medic | UnitKind::SiegeTank => UnitKind::Barracks,
                    _ => return,
                };

                // Prefer selected producer, otherwise fallback to any matching building.
                let target_idx = self
                    .units
                    .iter()
                    .position(|u| {
                        u.kind == producer_kind
                            && self.selected_ids.contains(&u.id)
                            && u.production_queue.len() < 5
                    })
                    .or_else(|| {
                        self.units
                            .iter()
                            .position(|u| u.kind == producer_kind && u.production_queue.len() < 5)
                    });

                if let Some(idx) = target_idx {
                    let (min, gas, _) = kind.cost();
                    self.minerals -= min;
                    self.gas -= gas;
                    self.units[idx].production_queue.push(kind);
                    self.status_banner = format!("Training {}...", kind.name());
                }
            }

            RtsMessage::CancelProduction(queue_idx) => {
                let selected_building = self
                    .units
                    .iter_mut()
                    .find(|u| self.selected_ids.contains(&u.id) && u.kind.is_building());
                if let Some(b) = selected_building {
                    if queue_idx < b.production_queue.len() {
                        let canceled = b.production_queue.remove(queue_idx);
                        if queue_idx == 0 {
                            b.production_elapsed = 0.0;
                        }
                        let (min, gas, _) = canceled.cost();
                        self.minerals += min;
                        self.gas += gas;
                        self.status_banner = format!("Canceled {}", canceled.name());
                    }
                }
            }

            RtsMessage::TogglePause => {
                self.paused = !self.paused;
                self.status_banner = if self.paused {
                    "Simulation Paused".to_string()
                } else {
                    "Simulation Running".to_string()
                };
            }
        }
    }

    fn view(&self) -> Widget<Self::Message> {
        let world_units: Vec<WorldUnitOverlay> = self
            .units
            .iter()
            .map(|u| WorldUnitOverlay {
                id: u.id,
                pos: u.pos,
                radius: u.kind.radius(),
                label: if u.stim_timer > 0.0 {
                    format!("{} [STIM]", u.kind.name())
                } else {
                    u.kind.name().to_string()
                },
                hp: u.hp,
                max_hp: u.max_hp,
                shield: u.energy,
                production_progress: u.active_production_fraction(),
                selected: self.selected_ids.contains(&u.id),
                color: u.kind.color(),
                is_building: u.kind.is_building(),
            })
            .collect();

        let world_canvas = WorldCanvas {
            units: world_units,
            order_markers: self.order_markers.clone(),
            on_select_unit: Some(Arc::new(RtsMessage::SelectUnit)),
            on_box_select: Some(Arc::new(RtsMessage::BoxSelect)),
            on_right_click: Some(Arc::new(RtsMessage::IssueMoveOrder)),
        };

        Widget::stack(vec![
            // Layer 0: Battlefield Canvas
            StackChild::NonPositioned(Widget::world_canvas(world_canvas)),
            // Layer 1a: Top Resource & Telemetry Bar
            StackChild::Positioned {
                left: Some(16.0),
                top: Some(12.0),
                right: Some(16.0),
                bottom: None,
                width: None,
                height: Some(46.0),
                child: self.build_top_bar(),
            },
            // Layer 1b: Bottom-Left Tactical Minimap Panel
            StackChild::Positioned {
                left: Some(16.0),
                top: None,
                right: None,
                bottom: Some(16.0),
                width: Some(236.0),
                height: Some(200.0),
                child: self.build_minimap_panel(),
            },
            // Layer 1c: Bottom-Center Selection & Production Panel
            StackChild::Positioned {
                left: Some(268.0),
                top: None,
                right: Some(348.0),
                bottom: Some(16.0),
                width: None,
                height: Some(200.0),
                child: self.build_selection_panel(),
            },
            // Layer 1d: Bottom-Right 4x3 Command Card
            StackChild::Positioned {
                left: None,
                top: None,
                right: Some(16.0),
                bottom: Some(16.0),
                width: Some(316.0),
                height: Some(200.0),
                child: self.build_command_card(),
            },
        ])
    }
}

impl RtsDemoApp {
    fn build_top_bar(&self) -> Widget<RtsMessage> {
        let mins = (self.elapsed_time as u32) / 60;
        let secs = (self.elapsed_time as u32) % 60;
        let pointer_label = if self.hud_wants_pointer {
            "PTR: HUD"
        } else {
            "PTR: BATTLEFIELD"
        };
        let pointer_color = if self.hud_wants_pointer {
            Color::ACCENT_GOLD
        } else {
            Color::HEALTH_GREEN
        };

        Widget::container()
            .id("top_resource_bar")
            .padding(EdgeInsets::symmetric(8.0, 14.0))
            .decoration(BoxDecoration::hud_panel())
            .block_pointer(true)
            .child(Widget::row(
                MainAxisAlignment::SpaceBetween,
                CrossAxisAlignment::Center,
                16.0,
                vec![
                    FlexChild::fixed(Widget::row(
                        MainAxisAlignment::Start,
                        CrossAxisAlignment::Center,
                        20.0,
                        vec![
                            FlexChild::fixed(Widget::text(
                                format!("MINERALS: {}", self.minerals),
                                TextStyle::new(14.0, Color::MINERAL_BLUE),
                            )),
                            FlexChild::fixed(Widget::text(
                                format!("GAS: {}", self.gas),
                                TextStyle::new(14.0, Color::GAS_GREEN),
                            )),
                            FlexChild::fixed(Widget::text(
                                format!("SUPPLY: {}/{}", self.supply_used(), self.supply_cap),
                                TextStyle::new(14.0, Color::ACCENT_GOLD),
                            )),
                            FlexChild::fixed(Widget::text(
                                format!("TIME {:02}:{:02}", mins, secs),
                                TextStyle::new(13.0, Color::WHITE),
                            )),
                        ],
                    )),
                    FlexChild::fixed(Widget::row(
                        MainAxisAlignment::End,
                        CrossAxisAlignment::Center,
                        12.0,
                        vec![
                            FlexChild::fixed(Widget::text(
                                self.status_banner.clone(),
                                TextStyle::new(12.5, Color::ACCENT_CYAN),
                            )),
                            FlexChild::fixed(Widget::text(
                                pointer_label,
                                TextStyle::new(12.0, pointer_color),
                            )),
                            FlexChild::fixed(Widget::sized_box(
                                Some(86.0),
                                Some(28.0),
                                Some(Widget::button(
                                    ButtonWidget::new(
                                        "btn_pause",
                                        if self.paused { "Resume" } else { "Pause" },
                                    )
                                    .with_hotkey(KeyCode::Space)
                                    .with_selected(self.paused)
                                    .with_on_press(RtsMessage::TogglePause)
                                    .with_tooltip(
                                        TooltipSpec::new(
                                            "Pause / Resume Simulation",
                                            "Toggles the RTS simulation tick timer.",
                                        )
                                        .with_hotkey("Space"),
                                    ),
                                )),
                            )),
                        ],
                    )),
                ],
            ))
            .build()
    }

    fn build_minimap_panel(&self) -> Widget<RtsMessage> {
        let mobile_count = self.units.iter().filter(|u| !u.kind.is_building()).count();
        let building_count = self.units.iter().filter(|u| u.kind.is_building()).count();

        Widget::container()
            .id("minimap_panel")
            .padding(EdgeInsets::all(10.0))
            .decoration(BoxDecoration::hud_panel())
            .block_pointer(true)
            .tooltip(TooltipSpec::new(
                "Tactical Radar",
                "Shows sector telemetry. Pointer clicks here are blocked from hitting the battlefield.",
            ))
            .child(Widget::column(
                MainAxisAlignment::SpaceBetween,
                CrossAxisAlignment::Stretch,
                8.0,
                vec![
                    FlexChild::fixed(Widget::row(
                        MainAxisAlignment::SpaceBetween,
                        CrossAxisAlignment::Center,
                        4.0,
                        vec![
                            FlexChild::fixed(Widget::text(
                                "TACTICAL RADAR",
                                TextStyle::new(12.5, Color::ACCENT_GOLD),
                            )),
                            FlexChild::fixed(Widget::text(
                                "SEC-04",
                                TextStyle::new(11.0, Color::ACCENT_CYAN),
                            )),
                        ],
                    )),
                    FlexChild::expanded(
                        1.0,
                        Widget::container()
                            .decoration(BoxDecoration {
                                bg_color: Color::rgba(0.025, 0.045, 0.065, 0.96),
                                border_color: Color::HUD_BORDER,
                                border_width: 1.0,
                                corner_radius: 4.0,
                            })
                            .alignment(Alignment::CENTER)
                            .child(Widget::column(
                                MainAxisAlignment::Center,
                                CrossAxisAlignment::Center,
                                6.0,
                                vec![
                                    FlexChild::fixed(Widget::text(
                                        format!("Units Active: {mobile_count}"),
                                        TextStyle::new(12.5, Color::HEALTH_GREEN),
                                    )),
                                    FlexChild::fixed(Widget::text(
                                        format!("Structures: {building_count}"),
                                        TextStyle::new(12.0, Color::MINERAL_BLUE),
                                    )),
                                    FlexChild::fixed(Widget::text(
                                        format!("Orders: {}", self.order_markers.len()),
                                        TextStyle::new(11.5, Color::WHITE),
                                    )),
                                ],
                            ))
                            .build(),
                    ),
                ],
            ))
            .build()
    }

    fn build_selection_panel(&self) -> Widget<RtsMessage> {
        let selected = self.selected_units();

        let inner = if selected.is_empty() {
            Widget::container()
                .alignment(Alignment::CENTER)
                .child(Widget::column(
                    MainAxisAlignment::Center,
                    CrossAxisAlignment::Center,
                    8.0,
                    vec![
                        FlexChild::fixed(Widget::text(
                            "NO UNITS SELECTED",
                            TextStyle::new(15.0, Color::ACCENT_GOLD),
                        )),
                        FlexChild::fixed(Widget::text(
                            "Left-click or drag-box on the battlefield to select units.",
                            TextStyle::new(12.5, Color::WHITE),
                        )),
                        FlexChild::fixed(Widget::text(
                            "Right-click ground to issue move orders. Hover buttons for tooltips.",
                            TextStyle::new(12.0, Color::ACCENT_CYAN),
                        )),
                    ],
                ))
                .build()
        } else if selected.len() == 1 {
            let u = selected[0];
            let hp_frac = (u.hp / u.max_hp).clamp(0.0, 1.0);
            let hp_color = if hp_frac > 0.55 {
                Color::HEALTH_GREEN
            } else if hp_frac > 0.25 {
                Color::HEALTH_YELLOW
            } else {
                Color::HEALTH_RED
            };

            let mut info_rows = vec![
                FlexChild::fixed(Widget::text(
                    u.kind.name().to_uppercase(),
                    TextStyle::new(16.0, Color::ACCENT_GOLD),
                )),
                FlexChild::fixed(Widget::text(
                    u.kind.weapon_info(),
                    TextStyle::new(12.0, Color::rgba(0.82, 0.88, 0.96, 0.95)),
                )),
                FlexChild::fixed(Widget::progress_bar(
                    hp_frac,
                    hp_color,
                    Color::rgba(0.03, 0.04, 0.06, 0.95),
                    Color::HUD_BORDER,
                    None,
                    18.0,
                    Some(format!("HP {:.0} / {:.0}", u.hp, u.max_hp)),
                )),
            ];

            if let Some((e, max_e)) = u.energy {
                info_rows.push(FlexChild::fixed(Widget::progress_bar(
                    (e / max_e).clamp(0.0, 1.0),
                    Color::SHIELD_BLUE,
                    Color::rgba(0.03, 0.04, 0.06, 0.95),
                    Color::HUD_BORDER,
                    None,
                    16.0,
                    Some(format!("ENERGY {:.0} / {:.0}", e, max_e)),
                )));
            }

            if u.kind.is_building() {
                if let Some(frac) = u.active_production_fraction() {
                    let training_name = u
                        .production_queue
                        .first()
                        .map(|k| k.name())
                        .unwrap_or("Unit");
                    info_rows.push(FlexChild::fixed(Widget::progress_bar(
                        frac,
                        Color::ACCENT_GOLD,
                        Color::rgba(0.03, 0.04, 0.06, 0.95),
                        Color::ACCENT_GOLD.with_alpha(0.6),
                        None,
                        18.0,
                        Some(format!("Training {training_name} ({:.0}%)", frac * 100.0)),
                    )));
                    let q_buttons: Vec<Widget<RtsMessage>> = u
                        .production_queue
                        .iter()
                        .enumerate()
                        .take(5)
                        .map(|(qi, qk)| {
                            Widget::button(
                                ButtonWidget::new(&format!("q_slot_{qi}"), qk.name())
                                    .with_on_press(RtsMessage::CancelProduction(qi))
                                    .with_tooltip(TooltipSpec::new(
                                        format!("Cancel {}", qk.name()),
                                        "Click to cancel queued production and refund resources.",
                                    )),
                            )
                        })
                        .collect();
                    info_rows.push(FlexChild::fixed(Widget::grid(5, 76.0, 28.0, 6.0, q_buttons)));
                } else {
                    info_rows.push(FlexChild::fixed(Widget::text(
                        "Production Idle - Use Command Card to train units.",
                        TextStyle::new(12.0, Color::ACCENT_CYAN),
                    )));
                }
            }

            Widget::row(
                MainAxisAlignment::Start,
                CrossAxisAlignment::Stretch,
                14.0,
                vec![
                    FlexChild::fixed(
                        Widget::container()
                            .size(96.0, 172.0)
                            .decoration(BoxDecoration {
                                bg_color: u.kind.color().with_alpha(0.25),
                                border_color: u.kind.color(),
                                border_width: 2.0,
                                corner_radius: 6.0,
                            })
                            .alignment(Alignment::CENTER)
                            .child(Widget::text(
                                format!("#{}", u.id),
                                TextStyle::new(20.0, Color::WHITE),
                            ))
                            .build(),
                    ),
                    FlexChild::expanded(
                        1.0,
                        Widget::column(
                            MainAxisAlignment::Start,
                            CrossAxisAlignment::Stretch,
                            8.0,
                            info_rows,
                        ),
                    ),
                ],
            )
        } else {
            let cards: Vec<Widget<RtsMessage>> = selected
                .iter()
                .take(12)
                .map(|u| {
                    let hp_pct = (u.hp / u.max_hp * 100.0).round() as u32;
                    Widget::button(
                        ButtonWidget::new(&format!("sel_card_{}", u.id), u.kind.name())
                            .with_sub_label(format!("{hp_pct}% HP"))
                            .with_on_press(RtsMessage::FocusUnitFromGroup(u.id))
                            .with_tooltip(TooltipSpec::new(
                                format!("{} #{}", u.kind.name(), u.id),
                                "Click to focus selection on this individual unit.",
                            )),
                    )
                })
                .collect();

            Widget::column(
                MainAxisAlignment::Start,
                CrossAxisAlignment::Stretch,
                8.0,
                vec![
                    FlexChild::fixed(Widget::row(
                        MainAxisAlignment::SpaceBetween,
                        CrossAxisAlignment::Center,
                        8.0,
                        vec![
                            FlexChild::fixed(Widget::text(
                                format!("SELECTED SQUAD ({} UNITS)", selected.len()),
                                TextStyle::new(13.5, Color::ACCENT_GOLD),
                            )),
                            FlexChild::fixed(Widget::text(
                                "Click unit card to focus",
                                TextStyle::new(11.5, Color::ACCENT_CYAN),
                            )),
                        ],
                    )),
                    FlexChild::expanded(1.0, Widget::grid(4, 134.0, 44.0, 8.0, cards)),
                ],
            )
        };

        Widget::container()
            .id("selection_panel")
            .padding(EdgeInsets::all(12.0))
            .decoration(BoxDecoration::hud_panel())
            .block_pointer(true)
            .child(inner)
            .build()
    }

    fn build_command_card(&self) -> Widget<RtsMessage> {
        let selected = self.selected_units();
        let has_mobile = selected.iter().any(|u| !u.kind.is_building());
        let has_marine = selected.iter().any(|u| u.kind == UnitKind::Marine);
        let max_stim_cd = selected
            .iter()
            .filter(|u| u.kind == UnitKind::Marine)
            .map(|u| u.stim_cooldown / 8.0)
            .fold(0.0_f32, f32::max);
        let stim_cd_opt = if max_stim_cd > 0.01 {
            Some(max_stim_cd)
        } else {
            None
        };

        let can_scv = self.can_afford(UnitKind::Scv);
        let can_marine = self.can_afford(UnitKind::Marine);
        let can_medic = self.can_afford(UnitKind::Medic);
        let can_tank = self.can_afford(UnitKind::SiegeTank);

        let empty_slot = |idx: usize| {
            Widget::button(
                ButtonWidget::new(&format!("cmd_empty_{idx}"), "--")
                    .with_enabled(false)
                    .with_style(ButtonStyle {
                        disabled_bg: Color::rgba(0.04, 0.05, 0.08, 0.45),
                        border_color: Color::HUD_BORDER.with_alpha(0.35),
                        ..ButtonStyle::default()
                    }),
            )
        };

        let slots = vec![
            // Row 1: Unit Movement & Combat Orders
            Widget::button(
                ButtonWidget::new("cmd_move", "Move")
                    .with_hotkey(KeyCode::Q)
                    .with_enabled(has_mobile)
                    .with_selected(self.active_command == Some(UnitAction::MoveMode))
                    .with_on_press(RtsMessage::UnitCommand(UnitAction::MoveMode))
                    .with_tooltip(
                        TooltipSpec::new("Move", "Orders selected units to move to target location.")
                            .with_hotkey("Q"),
                    ),
            ),
            Widget::button(
                ButtonWidget::new("cmd_stop", "Stop")
                    .with_hotkey(KeyCode::W)
                    .with_enabled(has_mobile)
                    .with_on_press(RtsMessage::UnitCommand(UnitAction::Stop))
                    .with_tooltip(
                        TooltipSpec::new("Stop", "Halts current movement and clears active orders.")
                            .with_hotkey("W"),
                    ),
            ),
            Widget::button(
                ButtonWidget::new("cmd_hold", "Hold")
                    .with_hotkey(KeyCode::E)
                    .with_enabled(has_mobile)
                    .with_selected(self.active_command == Some(UnitAction::HoldPosition))
                    .with_on_press(RtsMessage::UnitCommand(UnitAction::HoldPosition))
                    .with_tooltip(
                        TooltipSpec::new(
                            "Hold Position",
                            "Units hold ground and refuse to chase targets.",
                        )
                        .with_hotkey("E"),
                    ),
            ),
            Widget::button(
                ButtonWidget::new("cmd_attack", "Attack")
                    .with_hotkey(KeyCode::A)
                    .with_enabled(has_mobile)
                    .with_selected(self.active_command == Some(UnitAction::AttackMove))
                    .with_on_press(RtsMessage::UnitCommand(UnitAction::AttackMove))
                    .with_tooltip(
                        TooltipSpec::new(
                            "Attack Move",
                            "Advances toward target while engaging enemies along the way.",
                        )
                        .with_hotkey("A"),
                    ),
            ),
            // Row 2: Abilities
            Widget::button(
                ButtonWidget::new("cmd_stim", "Stimpack")
                    .with_sub_label(if stim_cd_opt.is_some() {
                        "Cooldown"
                    } else {
                        "-10 HP"
                    })
                    .with_hotkey(KeyCode::R)
                    .with_enabled(has_marine)
                    .with_cooldown(stim_cd_opt)
                    .with_on_press(RtsMessage::UnitCommand(UnitAction::TriggerStimpack))
                    .with_tooltip(
                        TooltipSpec::new(
                            "Stimpack",
                            "Boosts Marine movement and attack speed by 65% for 5s at the cost of 10 HP.",
                        )
                        .with_hotkey("R")
                        .with_cooldown(8.0),
                    ),
            ),
            Widget::button(
                ButtonWidget::new("cmd_repair", "Repair")
                    .with_sub_label("+20 HP")
                    .with_hotkey(KeyCode::T)
                    .with_enabled(has_mobile)
                    .with_on_press(RtsMessage::UnitCommand(UnitAction::NanoRepair))
                    .with_tooltip(
                        TooltipSpec::new(
                            "Nano-Repair",
                            "Applies emergency field nano-repair (+20 HP) to selected units.",
                        )
                        .with_hotkey("T"),
                    ),
            ),
            empty_slot(6),
            empty_slot(7),
            // Row 3: Production & Clear
            Widget::button(
                ButtonWidget::new("cmd_train_scv", "SCV")
                    .with_sub_label("50m")
                    .with_hotkey(KeyCode::S)
                    .with_enabled(can_scv)
                    .with_on_press(RtsMessage::TrainUnit(UnitKind::Scv))
                    .with_tooltip(
                        TooltipSpec::new(
                            "Train SCV",
                            "Worker unit that harvests Minerals and constructs buildings.",
                        )
                        .with_hotkey("S")
                        .with_minerals(50, self.minerals >= 50),
                    ),
            ),
            Widget::button(
                ButtonWidget::new("cmd_train_marine", "Marine")
                    .with_sub_label("50m")
                    .with_hotkey(KeyCode::D)
                    .with_enabled(can_marine)
                    .with_on_press(RtsMessage::TrainUnit(UnitKind::Marine))
                    .with_tooltip(
                        TooltipSpec::new(
                            "Train Marine",
                            "Versatile light infantry armed with a C-14 Gauss Rifle.",
                        )
                        .with_hotkey("D")
                        .with_minerals(50, self.minerals >= 50),
                    ),
            ),
            Widget::button(
                ButtonWidget::new("cmd_train_tank", "Tank")
                    .with_sub_label("150/125")
                    .with_hotkey(KeyCode::F)
                    .with_enabled(can_tank)
                    .with_on_press(RtsMessage::TrainUnit(UnitKind::SiegeTank))
                    .with_tooltip(
                        TooltipSpec::new(
                            "Build Siege Tank",
                            "Heavy armored artillery vehicle with high-impact 90mm cannon.",
                        )
                        .with_hotkey("F")
                        .with_minerals(150, self.minerals >= 150)
                        .with_gas(125, self.gas >= 125),
                    ),
            ),
            Widget::button(
                ButtonWidget::new("cmd_clear", "Clear")
                    .with_hotkey(KeyCode::Escape)
                    .with_enabled(!self.selected_ids.is_empty())
                    .with_on_press(RtsMessage::ClearSelection)
                    .with_tooltip(
                        TooltipSpec::new("Clear Selection", "Deselects all active units.")
                            .with_hotkey("Esc"),
                    ),
            ),
        ];

        let _ = can_medic; // Kept available for future command card pages.

        Widget::container()
            .id("command_card_panel")
            .padding(EdgeInsets::all(10.0))
            .decoration(BoxDecoration::hud_panel())
            .block_pointer(true)
            .child(Widget::column(
                MainAxisAlignment::Start,
                CrossAxisAlignment::Stretch,
                6.0,
                vec![
                    FlexChild::fixed(Widget::text(
                        "COMMAND CARD",
                        TextStyle::new(12.0, Color::ACCENT_GOLD),
                    )),
                    FlexChild::expanded(1.0, Widget::grid(4, 70.0, 50.0, 5.0, slots)),
                ],
            ))
            .build()
    }
}
