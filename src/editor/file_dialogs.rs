//! Sélecteurs de fichiers **asynchrones** de l'éditeur (roadmap post-audit UX
//! v2 2026-09-04, 6.1).
//!
//! Avant : chaque « Choisir… » appelait `rfd::FileDialog::…pick_file()` depuis
//! une closure egui — appel bloquant sur le thread de rendu, donc une fenêtre
//! figée (plus une frame dessinée, spinner immobile, autosave et réseau gelés)
//! tant que le sélecteur natif restait ouvert.
//!
//! Maintenant : la demande est décrite par une [`DialogRequest`] (type de
//! sélecteur, filtres) et une [`DialogTarget`] (où va le chemin choisi). Le
//! futur `rfd::AsyncFileDialog` est **créé sur le thread principal** (sur
//! macOS la feuille modale doit y naître) puis **attendu sur un thread de
//! fond** ; le résultat revient par un canal, relevé une fois par frame par
//! [`FileDialogs::poll`] et transformé en `UiActions`/mutation d'état par
//! `Editor::run`. La boucle de rendu continue de tourner pendant ce temps.
//!
//! Sur mobile/web (pas de sélecteur natif via `rfd`), `open` est un no-op :
//! même comportement qu'avant, aucun sélecteur ne s'ouvre.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

/// Genre de sélecteur natif à ouvrir.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DialogRequest {
    /// Un fichier existant, filtré par extensions (`(libellé, extensions)`).
    PickFile {
        filter: &'static str,
        extensions: &'static [&'static str],
    },
    /// Un fichier à écrire, avec un nom proposé.
    SaveFile {
        filter: &'static str,
        extensions: &'static [&'static str],
        file_name: String,
    },
    /// Un dossier.
    PickFolder,
}

/// Champ d'icône/splash du panneau d'export visé par un sélecteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExportAsset {
    Icon,
    Splash,
}

/// Destination du chemin choisi — reliée à l'action ou au champ correspondant
/// par `Editor::run` une fois le sélecteur refermé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DialogTarget {
    /// « Ouvrir… » : scène seule ou manifeste de projet (`menus::dialog_open`).
    OpenSceneOrProject,
    /// « Enregistrer sous… » (`UiActions::save_path`).
    SaveAs,
    /// « Ouvrir un projet… » : dossier racine (`UiActions::open_project_path`).
    OpenProjectFolder,
    /// « Localiser… » un projet récent introuvable : `forget` est l'ancien
    /// chemin à retirer des récents une fois le nouveau choisi.
    LocateRecent { forget: String },
    /// « Importer glTF… » (`UiActions::import`).
    ImportGltf,
    /// Texture de l'objet `index` (inspecteur).
    ObjectTexture { index: usize },
    /// Clip audio de l'objet `index` ; `normalize` = mesurer le gain de
    /// loudness à l'import (menu Ajouter, cf. Sprint 126), sinon garder le
    /// gain courant (inspecteur).
    ObjectAudio { index: usize, normalize: bool },
    /// Profil `.mobileprovision` du panneau d'export.
    IosProfile,
    /// Icône ou splash du panneau d'export.
    ExportAsset(ExportAsset),
    /// Emplacement du formulaire « Nouveau projet ».
    NewProjectLocation,
}

/// Un sélecteur ouvert, dont on attend la réponse.
struct Pending {
    target: DialogTarget,
    rx: Receiver<Option<PathBuf>>,
}

/// Sélecteurs en cours (cf. module). Vit dans `Editor`.
#[derive(Default)]
pub(super) struct FileDialogs {
    pending: Vec<Pending>,
}

impl FileDialogs {
    /// Ouvre un sélecteur natif sans bloquer ; le résultat arrivera par
    /// [`FileDialogs::poll`]. Un seul sélecteur à la fois : une demande faite
    /// pendant qu'un autre est ouvert est ignorée (le second clic d'un
    /// double-clic, par exemple).
    pub(super) fn open(&mut self, request: DialogRequest, target: DialogTarget) {
        if !self.pending.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        spawn_native_dialog(request, tx);
        self.pending.push(Pending { target, rx });
    }

    /// Réponses arrivées depuis le dernier appel : `(destination, chemin)` —
    /// `None` = sélecteur annulé. À appeler une fois par frame.
    pub(super) fn poll(&mut self) -> Vec<(DialogTarget, Option<PathBuf>)> {
        let mut done = Vec::new();
        self.pending.retain(|p| match p.rx.try_recv() {
            Ok(path) => {
                done.push((p.target.clone(), path));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            // Thread disparu sans répondre : traité comme une annulation.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                done.push((p.target.clone(), None));
                false
            }
        });
        done
    }

    /// Injecte une réponse comme si un sélecteur venait de se refermer
    /// (tests : pas de sélecteur natif dans un test unitaire).
    #[cfg(test)]
    pub(super) fn push_result(&mut self, target: DialogTarget, path: Option<PathBuf>) {
        let (tx, rx) = channel();
        let _ = tx.send(path);
        self.pending.push(Pending { target, rx });
    }
}

/// Crée le futur `rfd` **ici** (thread principal) et l'attend sur un thread de
/// fond — cf. module. `pollster` bloque ce thread de fond, jamais celui du rendu.
#[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
fn spawn_native_dialog(request: DialogRequest, tx: Sender<Option<PathBuf>>) {
    use std::future::Future;
    use std::pin::Pin;
    type PathFuture = Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>;
    let future: PathFuture = match request {
        DialogRequest::PickFile { filter, extensions } => {
            let fut = rfd::AsyncFileDialog::new()
                .add_filter(filter, extensions)
                .pick_file();
            Box::pin(async move { fut.await.map(|h| h.path().to_path_buf()) })
        }
        DialogRequest::SaveFile {
            filter,
            extensions,
            file_name,
        } => {
            let fut = rfd::AsyncFileDialog::new()
                .add_filter(filter, extensions)
                .set_file_name(file_name)
                .save_file();
            Box::pin(async move { fut.await.map(|h| h.path().to_path_buf()) })
        }
        DialogRequest::PickFolder => {
            let fut = rfd::AsyncFileDialog::new().pick_folder();
            Box::pin(async move { fut.await.map(|h| h.path().to_path_buf()) })
        }
    };
    std::thread::spawn(move || {
        let _ = tx.send(pollster::block_on(future));
    });
}

/// Pas de sélecteur natif sur ces cibles : le canal est refermé sans réponse,
/// `poll` le lit comme une annulation.
#[cfg(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))]
fn spawn_native_dialog(_request: DialogRequest, _tx: Sender<Option<PathBuf>>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_returns_each_answer_once_and_a_dropped_sender_reads_as_cancelled() {
        let mut dialogs = FileDialogs::default();
        assert!(dialogs.pending.is_empty());
        dialogs.push_result(DialogTarget::ImportGltf, Some(PathBuf::from("/tmp/a.glb")));
        assert_eq!(dialogs.pending.len(), 1);
        let got = dialogs.poll();
        assert_eq!(
            got,
            vec![(DialogTarget::ImportGltf, Some(PathBuf::from("/tmp/a.glb")))]
        );
        assert!(dialogs.poll().is_empty());
        assert!(dialogs.pending.is_empty());

        // Émetteur disparu sans réponse : annulation, pas d'attente infinie.
        let (_, rx) = channel::<Option<PathBuf>>();
        dialogs.pending.push(Pending {
            target: DialogTarget::SaveAs,
            rx,
        });
        assert_eq!(dialogs.poll(), vec![(DialogTarget::SaveAs, None)]);
    }

    #[test]
    fn only_one_dialog_at_a_time() {
        let mut dialogs = FileDialogs::default();
        dialogs.push_result(DialogTarget::SaveAs, None);
        // Un second sélecteur pendant que le premier est ouvert est ignoré.
        dialogs.open(DialogRequest::PickFolder, DialogTarget::OpenProjectFolder);
        assert_eq!(dialogs.pending.len(), 1);
        assert_eq!(dialogs.poll(), vec![(DialogTarget::SaveAs, None)]);
    }
}
