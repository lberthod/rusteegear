//! Événements d'entrée **agnostiques de la plateforme**.
//!
//! winit (desktop), le tactile (iOS/Android) ou la VR (OpenXR) traduisent leurs
//! événements natifs vers cet enum ; la logique applicative ne dépend que de lui.

#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    /// Début d'un appui (clic gauche / doigt posé).
    PointerDown,
    /// Fin d'un appui (relâchement).
    PointerUp,
    /// Déplacement du pointeur, en pixels physiques.
    PointerMove { x: f64, y: f64 },
    /// Molette / pinch : delta positif = zoom avant.
    Scroll { delta: f32 },
}
