//! Turning plain-language requests about the *interface* into layout tokens.
//!
//! This is the part that answers "сделай кнопки крупнее", "перенеси настройки
//! на главный экран" or "убери анимации" without a language model. A rule set
//! is the right tool here: the vocabulary is small and closed, the answers are
//! exact rather than plausible, and it costs nothing to run.
//!
//! Everything it produces is an arrangement of capabilities the launcher
//! already has. Nothing in this file can add a capability, and every value it
//! returns is an enum or a bounded number that [`Layout::clamp_in_place`]
//! re-checks afterwards.

use super::model::{
    Corners, LabelCase, Layout, MotionStyle, Placement, Section, Shortcut,
};

/// Accumulates layout requests, then produces a repaired [`Layout`].
#[derive(Debug, Clone, Default)]
pub struct LayoutIntent {
    scale: Option<f32>,
    density: Option<u16>,
    placement: Option<Placement>,
    corners: Option<Corners>,
    motion: Option<MotionStyle>,
    motion_scale: Option<f32>,
    label_case: Option<LabelCase>,
    icon_only: Option<bool>,
    order: Vec<Section>,
    hidden: Vec<Section>,
    shortcuts: Vec<Shortcut>,
    touched: Vec<String>,
}

impl LayoutIntent {
    fn note(&mut self, what: &str) {
        if !self.touched.iter().any(|w| w == what) {
            self.touched.push(what.to_string());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.touched.is_empty()
    }

    pub fn words(&self) -> &[String] {
        &self.touched
    }

    // --- size ---------------------------------------------------------------

    pub fn scale_up(&mut self) {
        self.scale = Some(self.scale.unwrap_or(1.0) + 0.12);
        self.note("крупный интерфейс");
    }

    pub fn scale_down(&mut self) {
        self.scale = Some(self.scale.unwrap_or(1.0) - 0.12);
        self.note("мелкий интерфейс");
    }

    pub fn set_scale(&mut self, value: f32) {
        self.scale = Some(value);
        self.note("размер интерфейса");
    }

    pub fn denser(&mut self) {
        self.density = Some((self.density.unwrap_or(16) - 2).max(12));
        self.note("плотная сетка");
    }

    pub fn airier(&mut self) {
        self.density = Some((self.density.unwrap_or(16) + 3).min(28));
        self.note("просторная сетка");
    }

    // --- placement ----------------------------------------------------------

    pub fn set_placement(&mut self, value: Placement, what: &str) {
        self.placement = Some(value);
        self.note(what);
    }

    // --- shape --------------------------------------------------------------

    pub fn set_corners(&mut self, value: Corners, what: &str) {
        self.corners = Some(value);
        self.note(what);
    }

    // --- motion -------------------------------------------------------------

    pub fn set_motion(&mut self, value: MotionStyle, what: &str) {
        self.motion = Some(value);
        self.note(what);
    }

    pub fn scale_motion(&mut self, factor: f32) {
        self.motion_scale = Some(factor);
        self.note("темп анимации");
    }

    // --- labels -------------------------------------------------------------

    pub fn set_label_case(&mut self, value: LabelCase, what: &str) {
        self.label_case = Some(value);
        self.note(what);
    }

    pub fn set_icon_only(&mut self, value: bool) {
        self.icon_only = Some(value);
        self.note(if value { "только значки" } else { "с подписями" });
    }

    // --- arrangement --------------------------------------------------------

    pub fn move_to_front(&mut self, section: Section) {
        self.order.retain(|s| *s != section);
        self.order.insert(0, section);
        self.hidden.retain(|s| *s != section);
        self.note("порядок меню");
    }

    pub fn hide(&mut self, section: Section) {
        if !self.hidden.contains(&section) {
            self.hidden.push(section);
        }
        self.note("скрытые разделы");
    }

    pub fn show(&mut self, section: Section) {
        self.hidden.retain(|s| *s != section);
        self.note("видимые разделы");
    }

    pub fn add_shortcut(&mut self, shortcut: Shortcut) {
        if !self.shortcuts.contains(&shortcut) {
            self.shortcuts.push(shortcut);
        }
        self.note("кнопки на главном экране");
    }

    /// Build the layout, letting the shared repair pass have the final word.
    pub fn build(&self) -> Layout {
        let base = Layout::default();
        let mut layout = Layout {
            scale: self.scale.unwrap_or(base.scale),
            density: self.density.unwrap_or(base.density),
            order: if self.order.is_empty() { base.order.clone() } else { self.order.clone() },
            hidden: self.hidden.clone(),
            shortcuts: self.shortcuts.clone(),
            placement: self.placement.unwrap_or(base.placement),
            corners: self.corners.unwrap_or(base.corners),
            motion: self.motion.unwrap_or(base.motion),
            motion_scale: self.motion_scale.unwrap_or(base.motion_scale),
            label_case: self.label_case.unwrap_or(base.label_case),
            icon_only: self.icon_only.unwrap_or(base.icon_only),
        };
        layout.clamp_in_place();
        layout
    }
}

/// Word fragments that mean "bigger", "smaller", and so on.
///
/// Stems, not whole words: people type "крупнее", "побольше", "поменьше".
const BIG: &[&str] = &["крупн", "больш", "побольш", "огромн", "увелич", "растяни", "растянуть"];
const SMALL: &[&str] = &["мелк", "маленьк", "поменьш", "уменьш", "сжат", "компактн"];
const DENSE: &[&str] = &["плотн", "тесн", "компакт", "эконом"];
const AIRY: &[&str] = &["просторн", "воздух", "свободн", "свободнее", "широк", "распущен"];

/// Read a request about the interface out of a description.
pub fn parse_layout(text: &str) -> LayoutIntent {
    let mut intent = LayoutIntent::default();
    let has = |needle: &str| text.contains(needle);

    // --- size ---------------------------------------------------------------
    for w in BIG {
        if has(w) {
            intent.scale_up();
            break;
        }
    }
    for w in SMALL {
        if has(w) {
            intent.scale_down();
            break;
        }
    }
    for w in DENSE {
        if has(w) {
            intent.denser();
            break;
        }
    }
    for w in AIRY {
        if has(w) {
            intent.airier();
            break;
        }
    }

    // --- placement ----------------------------------------------------------
    if has("слев") || has("левый край") || has("прижать влево") {
        intent.set_placement(Placement::Left, "слева");
    } else if has("справ") || has("правый край") {
        intent.set_placement(Placement::Right, "справа");
    } else if has("снизу") || has("низ") || has("баннер") {
        intent.set_placement(Placement::Banner, "снизу");
    }

    // --- corners ------------------------------------------------------------
    if has("прямые угл") || has("острые угл") || has("без скруглен") || has("скруглен") && has("нет") {
        intent.set_corners(Corners::Sharp, "острые углы");
    } else if has("кругл") || has("таблетк") || has("пилюл") {
        intent.set_corners(Corners::Pill, "круглые углы");
    } else if has("скругл") || has("мягк") || has("плавн") {
        intent.set_corners(Corners::Rounded, "мягкие углы");
    }

    // --- motion -------------------------------------------------------------
    if has("без анимац") || has("статичн") || has("неподвижн") || has("замороз") {
        intent.set_motion(MotionStyle::Still, "без анимаций");
    } else if has("пружин") || has("отскок") || has("жив") || has("прыг") {
        intent.set_motion(MotionStyle::Springy, "пружинная анимация");
    } else if has("резк") || has("быстр") || has("рывк") {
        intent.set_motion(MotionStyle::Sharp, "резкие переходы");
    } else if has("плавн") && (has("анимац") || has("переход") || has("движен")) {
        intent.set_motion(MotionStyle::Smooth, "плавные переходы");
    }
    if has("медленн") {
        intent.scale_motion(1.6);
    } else if has("мгновенн") || has("замедл") && has("нет") {
        intent.scale_motion(0.4);
    }

    // --- labels -------------------------------------------------------------
    if has("капс") || has("прописн") || has("заглавн") || has("верхн регистр") {
        intent.set_label_case(LabelCase::Upper, "ПРОПИСНЫЕ");
    } else if has("с заглавн") || has("титульн") {
        intent.set_label_case(LabelCase::Title, "Заглавные слова");
    }
    if has("без подпис") || has("только значк") || has("только иконк") || has("минимализм интерфейса") {
        intent.set_icon_only(true);
    }

    // --- arrangement --------------------------------------------------------
    if has("настройки на главн") || has("настройки вперёд") || has("настройки впер") {
        intent.move_to_front(Section::Settings);
        intent.add_shortcut(Shortcut::Settings);
    }
    if has("оформлени") && (has("на главн") || has("быстр")) {
        intent.move_to_front(Section::Appearance);
        intent.add_shortcut(Shortcut::Appearance);
    }
    if has("сборки на главн") {
        intent.move_to_front(Section::Instances);
        intent.add_shortcut(Shortcut::Instances);
    }
    if has("журнал на главн") || has("лог на главн") {
        intent.add_shortcut(Shortcut::Log);
    }
    if has("случайн") && (has("тем") || has("тему")) {
        intent.add_shortcut(Shortcut::RandomTheme);
    }
    if has("смен") && has("фон") && (has("кнопк") || has("быстр")) {
        intent.add_shortcut(Shortcut::ToggleBackground);
    }
    if has("производительн") && (has("быстр") || has("главн")) {
        intent.add_shortcut(Shortcut::Performance);
    }
    if has("убери") && has("о лаунчере") {
        intent.hide(Section::About);
    }
    if has("убери сборки") || has("без сборок") {
        intent.hide(Section::Instances);
    }
    if has("покажи всё") || has("верни всё") || has("покажи все разделы") {
        intent.hidden.clear();
        intent.note("видимые разделы");
    }

    intent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_requests_move_in_the_right_direction() {
        assert!(parse_layout("крупные кнопки").scale.unwrap_or(1.0) > 1.0);
        assert!(parse_layout("мелкие кнопки").scale.unwrap_or(1.0) < 1.0);
        assert!(parse_layout("плотная сетка").density.unwrap_or(16) < 16);
        assert!(parse_layout("просторная сетка").density.unwrap_or(16) > 16);
    }

    #[test]
    fn functions_can_be_promoted_to_the_home_screen() {
        let intent = parse_layout("перенеси настройки на главный экран");
        assert_eq!(intent.order.first(), Some(&Section::Settings));
        assert!(intent.shortcuts.contains(&Shortcut::Settings));

        let intent = parse_layout("журнал на главном экране");
        assert!(intent.shortcuts.contains(&Shortcut::Log));
    }

    #[test]
    fn motion_can_be_removed_or_given_a_character() {
        assert_eq!(parse_layout("без анимаций").motion, Some(MotionStyle::Still));
        assert_eq!(parse_layout("статичная тема").motion, Some(MotionStyle::Still));
        assert_eq!(parse_layout("пружинная анимация").motion, Some(MotionStyle::Springy));
    }

    #[test]
    fn a_theme_can_hide_sections_and_undo_it() {
        assert!(parse_layout("убери о лаунчере").hidden.contains(&Section::About));
        let restored = parse_layout("убери о лаунчере. покажи всё");
        assert!(restored.hidden.is_empty(), "{:?}", restored.hidden);
    }

    /// The built layout must survive the shared repair pass unchanged: the
    /// parser is allowed to be sloppy, `clamp_in_place` is not.
    #[test]
    fn built_layouts_are_complete_and_bounded() {
        for prompt in [
            "крупные кнопки, перенеси настройки на главный экран, без анимаций",
            "убери о лаунчере",
            "пружинная анимация, круглые углы, прописные буквы",
            "",
        ] {
            let layout = parse_layout(prompt).build();
            let mut order = layout.order.clone();
            order.sort_by_key(|s| Section::ALL.iter().position(|x| x == s).unwrap_or(99));
            assert_eq!(order, Section::ALL.to_vec(), "«{prompt}» left the menu incomplete");
            assert!((0.8..=1.4).contains(&layout.scale));
            assert!((12..=28).contains(&layout.density));
            assert!(layout.shortcuts.len() <= 6);
        }
    }

    #[test]
    fn an_empty_description_changes_nothing() {
        let intent = parse_layout("");
        assert!(intent.is_empty(), "{:?}", intent.words());
        assert_eq!(intent.build(), Layout::default());
    }
}