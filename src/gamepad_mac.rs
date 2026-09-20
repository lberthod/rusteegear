//! Manettes sur macOS via le framework Apple **GameController**.
//!
//! La lecture HID brute de `gilrs` décode mal une Switch Pro en Bluetooth : macOS la passe en
//! mode complet et les données de mouvement (200 Hz) sont lues comme des boutons — des milliers
//! d'événements parasites, des boutons A/B/X/Y qui « clignotent », une gâchette bloquée à 0,38.
//! GameController connaît ces manettes (Switch Pro, Joy-Con, Xbox, PlayStation) et rend un état
//! propre ; on le convertit vers les types de `gilrs` que le reste du moteur consomme déjà.
//!
//! Positions des boutons de façade : Apple nomme A/B/X/Y **par position** (A = bas, B = droite,
//! X = gauche, Y = haut), comme `gilrs` (South/East/West/North) — la Switch (B en bas, A à
//! droite) donne donc South = B, East = A, comme avec l'ancien chemin.

use std::collections::HashSet;

use gilrs::Button;
use objc2_game_controller::{GCController, GCControllerButtonInput, GCDevice};

/// État d'une manette pour une frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MacPad {
    pub name: String,
    pub held: HashSet<Button>,
    /// Stick gauche (x droite +, y haut +, comme `gilrs`).
    pub left: (f32, f32),
    pub right: (f32, f32),
    /// Gâchettes ZL/LT et ZR/RT, 0..1.
    pub lt: f32,
    pub rt: f32,
}

/// À appeler une fois : reçoit aussi les entrées quand la fenêtre n'est pas au premier plan.
pub fn init() {
    // SAFETY : propriété de classe, appel sur le thread principal avant la boucle d'événements.
    unsafe {
        GCController::setShouldMonitorBackgroundEvents(true);
    }
}

fn pressed(b: &GCControllerButtonInput) -> bool {
    // SAFETY : lecture d'une propriété d'un objet GameController retenu.
    unsafe { b.isPressed() }
}

/// Lit toutes les manettes « gamepad étendu » connectées (dans l'ordre de connexion).
pub fn read_all() -> Vec<MacPad> {
    // SAFETY : API GameController appelée depuis le thread principal ; les objets renvoyés
    // sont retenus le temps de la lecture.
    unsafe {
        let controllers = GCController::controllers();
        let mut out = Vec::new();
        for c in controllers.iter() {
            let Some(pad) = c.extendedGamepad() else {
                continue;
            };
            let mut held = HashSet::new();
            let mut add = |b: &GCControllerButtonInput, btn: Button| {
                if pressed(b) {
                    held.insert(btn);
                }
            };
            add(&pad.buttonA(), Button::South);
            add(&pad.buttonB(), Button::East);
            add(&pad.buttonX(), Button::West);
            add(&pad.buttonY(), Button::North);
            add(&pad.leftShoulder(), Button::LeftTrigger);
            add(&pad.rightShoulder(), Button::RightTrigger);
            add(&pad.leftTrigger(), Button::LeftTrigger2);
            add(&pad.rightTrigger(), Button::RightTrigger2);
            add(&pad.buttonMenu(), Button::Start);
            if let Some(b) = pad.buttonOptions() {
                add(&b, Button::Select);
            }
            if let Some(b) = pad.leftThumbstickButton() {
                add(&b, Button::LeftThumb);
            }
            if let Some(b) = pad.rightThumbstickButton() {
                add(&b, Button::RightThumb);
            }
            let dpad = pad.dpad();
            add(&dpad.up(), Button::DPadUp);
            add(&dpad.down(), Button::DPadDown);
            add(&dpad.left(), Button::DPadLeft);
            add(&dpad.right(), Button::DPadRight);
            let ls = pad.leftThumbstick();
            let rs = pad.rightThumbstick();
            let name = c
                .vendorName()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "Manette".to_string());
            out.push(MacPad {
                name,
                held,
                left: (ls.xAxis().value(), ls.yAxis().value()),
                right: (rs.xAxis().value(), rs.yAxis().value()),
                lt: pad.leftTrigger().value(),
                rt: pad.rightTrigger().value(),
            });
        }
        out
    }
}
