//! Correspondance entre le monde de course et les objets de la scène : quels indices
//! d'objets forment la voiture, le fantôme, les portiques… Pure donnée, remplie par
//! `Scene::herroad_demo` et lue par `app::race`.

use glam::{Quat, Vec3};

/// Une pièce de la voiture : posée à `offset` dans le repère du véhicule (X = gauche,
/// Y = haut, Z = avant), tournée de `local_rot` avant l'orientation de la caisse.
#[derive(Clone, Copy, Debug)]
pub struct CarPart {
    pub index: usize,
    pub offset: Vec3,
    pub local_rot: Quat,
    /// Roue avant : suit en plus le braquage.
    pub front_wheel: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RaceLayout {
    /// Relief autour du circuit (maillage, placement du décor, détection de chute).
    pub terrain: super::terrain::Terrain,
    pub car: Vec<CarPart>,
    pub ghost: Vec<CarPart>,
    /// Objets de chaque portique : points de passage puis, en dernier, la ligne d'arrivée.
    pub gates: Vec<Vec<usize>>,
    /// Émetteur de fumée de dérapage (suit la voiture).
    pub smoke: Option<usize>,
    /// Émetteur de flamme de turbo.
    pub flame: Option<usize>,
    /// Lumière ponctuelle qui éclaire la route devant la voiture.
    pub headlight: Option<usize>,
}
