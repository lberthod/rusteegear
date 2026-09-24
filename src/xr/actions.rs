//! Manettes Meta Quest Touch via les **actions OpenXR** (phase 3, APK seulement) :
//! un jeu d'actions « gameplay » (poses, gâchettes, grips, sticks, boutons,
//! vibrations), des correspondances suggérées pour les Touch et un profil
//! générique de repli, lu à chaque image en un `XrInput` — la même structure
//! que celle que produit le simulateur (`quest_sim`), consommée par
//! `xr::content` à l'identique.

use glam::{Quat, Vec3};
use openxr as xr;

use super::input::{HandInput, Haptic, LEFT, RIGHT, XrInput};

pub struct TouchActions {
    set: xr::ActionSet,
    hands: [xr::Path; 2],
    trigger: xr::Action<f32>,
    squeeze: xr::Action<f32>,
    stick: xr::Action<xr::Vector2f>,
    stick_click: xr::Action<bool>,
    primary: xr::Action<bool>,
    secondary: xr::Action<bool>,
    menu: xr::Action<bool>,
    haptic: xr::Action<xr::Haptic>,
    grip_spaces: [xr::Space; 2],
    aim_spaces: [xr::Space; 2],
    /// Suivi des mains (`XR_EXT_hand_tracking`, phase 8) si le runtime le
    /// propose — Quest : actif dès que le joueur pose les manettes.
    hand_trackers: Option<[xr::HandTracker; 2]>,
}

impl TouchActions {
    /// Crée les actions, suggère les correspondances et les attache à la
    /// session (à faire une fois, avant la première image).
    pub fn new(
        instance: &xr::Instance,
        session: &xr::Session<xr::Vulkan>,
        hand_tracking: bool,
    ) -> xr::Result<Self> {
        let path = |p: &str| instance.string_to_path(p);
        let hands = [path("/user/hand/left")?, path("/user/hand/right")?];
        let set = instance.create_action_set("gameplay", "Jeu", 0)?;
        let grip = set.create_action::<xr::Posef>("grip", "Poignée", &hands)?;
        let aim = set.create_action::<xr::Posef>("aim", "Visée", &hands)?;
        let trigger = set.create_action::<f32>("trigger", "Gâchette", &hands)?;
        let squeeze = set.create_action::<f32>("squeeze", "Grip", &hands)?;
        let stick = set.create_action::<xr::Vector2f>("stick", "Stick", &hands)?;
        let stick_click = set.create_action::<bool>("stick_click", "Clic du stick", &hands)?;
        let primary = set.create_action::<bool>("primary", "A / X", &hands)?;
        let secondary = set.create_action::<bool>("secondary", "B / Y", &hands)?;
        let menu = set.create_action::<bool>("menu", "Menu", &hands)?;
        let haptic = set.create_action::<xr::Haptic>("haptic", "Vibration", &hands)?;

        // Manettes Touch (Quest 2, 3, 3S, Pro).
        let touch = |a: &str, b: &str| -> xr::Result<[xr::Path; 2]> {
            Ok([
                path(&format!("/user/hand/left/{a}"))?,
                path(&format!("/user/hand/right/{b}"))?,
            ])
        };
        let both = |sub: &str| touch(sub, sub);
        let grip_p = both("input/grip/pose")?;
        let aim_p = both("input/aim/pose")?;
        let trigger_p = both("input/trigger/value")?;
        let squeeze_p = both("input/squeeze/value")?;
        let stick_p = both("input/thumbstick")?;
        let click_p = both("input/thumbstick/click")?;
        let primary_p = touch("input/x/click", "input/a/click")?;
        let secondary_p = touch("input/y/click", "input/b/click")?;
        let haptic_p = both("output/haptic")?;
        let menu_p = path("/user/hand/left/input/menu/click")?;
        let mut touch_bindings = Vec::new();
        for i in [LEFT, RIGHT] {
            touch_bindings.extend([
                xr::Binding::new(&grip, grip_p[i]),
                xr::Binding::new(&aim, aim_p[i]),
                xr::Binding::new(&trigger, trigger_p[i]),
                xr::Binding::new(&squeeze, squeeze_p[i]),
                xr::Binding::new(&stick, stick_p[i]),
                xr::Binding::new(&stick_click, click_p[i]),
                xr::Binding::new(&primary, primary_p[i]),
                xr::Binding::new(&secondary, secondary_p[i]),
                xr::Binding::new(&haptic, haptic_p[i]),
            ]);
        }
        touch_bindings.push(xr::Binding::new(&menu, menu_p));
        instance.suggest_interaction_profile_bindings(
            path("/interaction_profiles/oculus/touch_controller")?,
            &touch_bindings,
        )?;

        // Repli générique (tout runtime OpenXR) : poses, « select » comme
        // gâchette, menu, vibration.
        let select_p = both("input/select/click")?;
        let simple_menu_p = both("input/menu/click")?;
        let mut simple = Vec::new();
        for i in [LEFT, RIGHT] {
            simple.extend([
                xr::Binding::new(&grip, grip_p[i]),
                xr::Binding::new(&aim, aim_p[i]),
                xr::Binding::new(&trigger, select_p[i]),
                xr::Binding::new(&menu, simple_menu_p[i]),
                xr::Binding::new(&haptic, haptic_p[i]),
            ]);
        }
        instance.suggest_interaction_profile_bindings(
            path("/interaction_profiles/khr/simple_controller")?,
            &simple,
        )?;

        session.attach_action_sets(&[&set])?;
        let space = |a: &xr::Action<xr::Posef>, hand: xr::Path| {
            a.create_space(session, hand, xr::Posef::IDENTITY)
        };
        let grip_spaces = [space(&grip, hands[LEFT])?, space(&grip, hands[RIGHT])?];
        let aim_spaces = [space(&aim, hands[LEFT])?, space(&aim, hands[RIGHT])?];
        let hand_trackers = if hand_tracking {
            match (
                session.create_hand_tracker(xr::Hand::LEFT),
                session.create_hand_tracker(xr::Hand::RIGHT),
            ) {
                (Ok(l), Ok(r)) => Some([l, r]),
                (Err(e), _) | (_, Err(e)) => {
                    log::warn!("VR : suivi des mains indisponible ({e})");
                    None
                }
            }
        } else {
            None
        };
        Ok(Self {
            set,
            hands,
            trigger,
            squeeze,
            stick,
            stick_click,
            primary,
            secondary,
            menu,
            haptic,
            grip_spaces,
            aim_spaces,
            hand_trackers,
        })
    }

    /// Synchronise et lit les manettes pour l'image affichée à `time`, poses
    /// dans l'espace `stage`. Une manette éteinte ou hors de vue des caméras
    /// donne des valeurs neutres et des poses `None`, jamais une erreur.
    pub fn read(
        &self,
        session: &xr::Session<xr::Vulkan>,
        stage: &xr::Space,
        time: xr::Time,
    ) -> XrInput {
        if let Err(e) = session.sync_actions(&[(&self.set).into()]) {
            log::warn!("VR : synchronisation des manettes impossible ({e})");
            return XrInput::default();
        }
        let mut input = XrInput::default();
        for i in [LEFT, RIGHT] {
            let hand = self.hands[i];
            let f = |a: &xr::Action<f32>| a.state(session, hand).map_or(0.0, |s| s.current_state);
            let b = |a: &xr::Action<bool>| a.state(session, hand).is_ok_and(|s| s.current_state);
            let stick = self
                .stick
                .state(session, hand)
                .map_or((0.0, 0.0), |s| (s.current_state.x, s.current_state.y));
            input.hands[i] = HandInput {
                grip: locate(&self.grip_spaces[i], stage, time),
                aim: locate(&self.aim_spaces[i], stage, time),
                trigger: f(&self.trigger),
                squeeze: f(&self.squeeze),
                stick,
                stick_click: b(&self.stick_click),
                primary: b(&self.primary),
                secondary: b(&self.secondary),
                menu: b(&self.menu),
            };
            if let Some(trackers) = &self.hand_trackers {
                input.hand_joints[i] = locate_joints(stage, &trackers[i], time);
            }
        }
        input
    }

    /// Joue les vibrations demandées par le jeu (`xr::input::haptics_from_fx`).
    pub fn vibrate(&self, session: &xr::Session<xr::Vulkan>, haptics: [Option<Haptic>; 2]) {
        for (i, h) in haptics.into_iter().enumerate() {
            let Some(h) = h else { continue };
            let event = xr::HapticVibration::new()
                .amplitude(h.amplitude.clamp(0.0, 1.0))
                .duration(xr::Duration::from_nanos((h.seconds * 1e9) as i64))
                .frequency(xr::FREQUENCY_UNSPECIFIED);
            if let Err(e) = self.haptic.apply_feedback(session, self.hands[i], &event) {
                log::warn!("VR : vibration impossible ({e})");
            }
        }
    }
}

/// Articulations d'une main suivie dans `stage` — `None` si la main n'est pas
/// suivie (manette en main, hors du champ des caméras) ou si le poignet n'est
/// pas localisé.
fn locate_joints(
    stage: &xr::Space,
    tracker: &xr::HandTracker,
    time: xr::Time,
) -> Option<super::hands::Joints> {
    let joints = stage.locate_hand_joints(tracker, time).ok()??;
    let valid = xr::SpaceLocationFlags::POSITION_VALID;
    if !joints[super::hands::WRIST].location_flags.contains(valid) {
        return None;
    }
    let mut out = [Vec3::ZERO; super::hands::OPENXR_JOINTS];
    for (dst, j) in out.iter_mut().zip(joints.iter()) {
        let p = j.pose.position;
        *dst = Vec3::new(p.x, p.y, p.z);
    }
    Some(out)
}

/// Pose d'un espace d'action dans `stage`, si le runtime la connaît.
fn locate(space: &xr::Space, stage: &xr::Space, time: xr::Time) -> Option<(Vec3, Quat)> {
    let loc = space.locate(stage, time).ok()?;
    let flags = loc.location_flags;
    if !flags.contains(xr::SpaceLocationFlags::POSITION_VALID)
        || !flags.contains(xr::SpaceLocationFlags::ORIENTATION_VALID)
    {
        return None;
    }
    let (p, o) = (loc.pose.position, loc.pose.orientation);
    Some((
        Vec3::new(p.x, p.y, p.z),
        Quat::from_xyzw(o.x, o.y, o.z, o.w),
    ))
}
