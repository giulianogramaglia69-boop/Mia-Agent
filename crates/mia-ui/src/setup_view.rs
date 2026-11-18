//! Schermata di primo avvio: scelta del provider e della chiave dentro
//! l'app, senza terminale.
//!
//! Quando Mia parte senza un modello configurato (niente `MIA_MODEL_KEY`
//! e niente `model.yaml`) mostra questa schermata invece delle tre
//! colonne: l'utente sceglie il provider, incolla la chiave, controlla
//! modello e URL base e preme "Salva e avvia Mia". La configurazione
//! finisce in `model.yaml` e l'app si riavvia da sola — alla ripartenza
//! il modello è già pronto.
//!
//! "Salta per ora" scrive un marcatore `.setup-skipped` nella cartella
//! dati e riavvia senza modello: Mia funziona comunque per preferenze,
//! fatti e pagine, e alle domande risponde che serve un modello. Per
//! rivedere la schermata basta cancellare il marcatore (o dare
//! `mia --setup` da terminale).

use std::path::PathBuf;

use sequoia_ui::prelude::*;

use crate::providers::ModelProvider;
use crate::setup::build_config;

const ERROR_RED: Color = Color::hex(0xFF453A);
const MUTED: Color = Color::hex(0x94A3B8);

/// La schermata di setup del primo avvio.
pub fn setup_view(data_dir: PathBuf) -> impl View {
    let provider = Signal::new(ModelProvider::Groq);
    let key = Signal::new(String::new());
    let model = Signal::new(ModelProvider::Groq.default_model().to_string());
    let base_url = Signal::new(ModelProvider::Groq.base_url().to_string());
    let error = Signal::new(String::new());

    let mut column = Column::new()
        .fill_size()
        .padding(40.0)
        .gap(10.0)
        .child(Text::new("Benvenuto in Mia").size(22.0).bold())
        .child(
            Text::new(
                "Per rispondere alle domande Mia ha bisogno di un modello. \
                 Scegline uno, incolla la chiave e sei pronto: la chiave resta \
                 solo su questo computer, in mia-data/model.yaml.",
            )
            .size(14.0)
            .color(MUTED),
        )
        .child(Text::new("Provider").size(14.0).bold());

    for p in ModelProvider::ALL {
        let label = if p.default_model().is_empty() {
            p.label().to_string()
        } else {
            format!("{} — {}", p.label(), p.default_model())
        };
        let provider_sig = provider.clone();
        let model_sig = model.clone();
        let base_sig = base_url.clone();
        column = column.child(
            Button::new(label)
                .style(ButtonStyle::SECONDARY)
                .on_click(move || {
                    provider_sig.set(p);
                    model_sig.set(p.default_model().to_string());
                    base_sig.set(p.base_url().to_string());
                }),
        );
    }

    let provider_hint = provider.clone();
    let provider_chosen = provider.clone();
    let key_field = key.clone();
    let model_field = model.clone();
    let base_field = base_url.clone();
    let error_text = error.clone();

    let save_provider = provider.clone();
    let save_key = key.clone();
    let save_model = model.clone();
    let save_base = base_url.clone();
    let save_error = error.clone();
    let save_data = data_dir.clone();

    let skip_error = error.clone();
    let skip_data = data_dir.clone();

    column
        .child(
            Text::with(move || {
                format!("Provider scelto: {}", provider_chosen.get().label())
            })
            .size(13.0)
            .bold(),
        )
        .child(
            Text::with(move || {
                let p = provider_hint.get();
                match p.key_url() {
                    Some(url) => format!("Crea la chiave qui: {url}"),
                    None if p == ModelProvider::Ollama => {
                        "Ollama gira in locale: nessuna chiave, nessun cloud.".to_string()
                    }
                    None => "Porta l'endpoint e la chiave tuoi.".to_string(),
                }
            })
            .size(13.0)
            .color(MUTED),
        )
        .child(Text::new("Chiave API").size(14.0).bold())
        .child(
            TextField::new(key_field).placeholder("Incolla qui la chiave API"),
        )
        .child(Text::new("Modello").size(14.0).bold())
        .child(TextField::new(model_field).placeholder("Modello"))
        .child(Text::new("URL base").size(14.0).bold())
        .child(TextField::new(base_field).placeholder("URL base dell'API"))
        .child(
            Text::with(move || error_text.get())
                .size(13.0)
                .color(ERROR_RED),
        )
        .child(
            Row::new()
                .gap(8.0)
                .child(
                    Button::new("Salva e avvia Mia")
                        .style(ButtonStyle::PRIMARY)
                        .on_click(move || {
                            match build_config(
                                save_provider.get_untracked(),
                                &save_key.get_untracked(),
                                &save_model.get_untracked(),
                                &save_base.get_untracked(),
                            ) {
                                Err(msg) => save_error.set(msg),
                                Ok(config) => match config.save(&save_data) {
                                    Err(err) => {
                                        save_error.set(format!("Non riesco a salvare: {err}"))
                                    }
                                    Ok(()) => restart(&save_error),
                                },
                            }
                        }),
                )
                .child(
                    Button::new("Salta per ora")
                        .style(ButtonStyle::GHOST)
                        .on_click(move || {
                            let _ = std::fs::create_dir_all(&skip_data);
                            let _ = std::fs::write(skip_data.join(".setup-skipped"), "");
                            restart(&skip_error);
                        }),
                ),
        )
}

/// Riavvia l'app con gli stessi argomenti: alla ripartenza il modello
/// è già configurato (o il setup è saltato). Se il riavvio fallisce,
/// mostra l'errore e l'app resta aperta.
fn restart(error: &Signal<String>) {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => {
            error.set(format!("Non riesco a riavviare Mia: {err}"));
            return;
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match std::process::Command::new(exe).args(args).spawn() {
        Ok(_) => std::process::exit(0),
        Err(err) => error.set(format!("Non riesco a riavviare Mia: {err}")),
    }
}
