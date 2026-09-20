//! Essai de bout en bout de l'enregistrement HerRoad (record + fantôme) : le pilote automatique
//! boucle une course, puis une nouvelle session doit retrouver record, meilleur tour et fantôme.
//! À lancer avec un `HOME` jetable pour ne pas toucher aux vraies données :
//! `HOME=/tmp/hr cargo run --release --example herroad_e2e`
use motor3derust::app::AppState;
use motor3derust::racing::bot::Bot;
use motor3derust::racing::Phase;

fn drive_to_the_end(app: &mut AppState) {
    let mut bot = Bot::new(3);
    app.input_state.race.throttle = 1.0;
    for _ in 0..60 * 200 {
        let inp = {
            let s = app.race.as_ref().unwrap();
            bot.drive(&s.car, &s.track)
        };
        app.input_state.race.throttle = inp.throttle.max(0.3);
        app.input_state.race.brake = inp.brake;
        app.input_state.race.steer = inp.steer;
        app.race_step(1.0 / 60.0);
        if app.race.as_ref().unwrap().race.phase == Phase::Finished {
            return;
        }
    }
    panic!("course non terminée");
}

fn main() {
    println!("HOME = {:?}", std::env::var("HOME"));
    let mut app = AppState::default();
    app.load_herroad_demo();
    assert!(app.race.as_ref().unwrap().race.ghost.is_empty(), "session vierge attendue");
    drive_to_the_end(&mut app);
    let total = app.race.as_ref().unwrap().race.total.unwrap();
    println!("course terminée en {total:.2} s");

    let mut again = AppState::default();
    again.load_herroad_demo();
    let r = &again.race.as_ref().unwrap().race;
    println!(
        "nouvelle session : record {:?}, meilleur tour {:?}, {} intermédiaires, fantôme de {} échantillons",
        r.best_total,
        r.best_lap,
        r.best_splits.len(),
        r.ghost.len()
    );
    assert!(r.best_total.is_some_and(|t| (t - total).abs() < 0.01));
    assert!(r.best_lap.is_some());
    assert!(r.ghost.len() > 60 * 60);
    println!("OK : record, meilleur tour et fantôme retrouvés");
}
