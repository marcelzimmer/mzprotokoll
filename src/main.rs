#![cfg_attr(windows, windows_subsystem = "windows")]
//! MZProtokoll - Meeting-Protokoll-Editor
//!
//! Linux App zum Erstellen und Exportieren von Meeting-Protokollen
//! [Markdown & PDF].
//!
//! Autor:   Marcel Zimmer
//! Web:     https://www.marcelzimmer.de
//! X:       https://x.com/marcelzimmer
//! GitHub:  https://github.com/marcelzimmer
//! Lizenz:  MIT
//! Version: 1.0.0
//! Datum:   05.02.2026

use chrono::{Datelike, Local, NaiveDate};
use eframe::egui::{self, RichText};
use genpdf::Element as _;
use std::collections::HashMap;
use std::sync::mpsc;

/// Öffnet eine URL im Standard-Webbrowser (Windows und Linux).
fn url_oeffnen(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Erstellt eine fette Schrift mit der angegebenen Größe (in Punkten).
fn fette_schrift(groesse: f32) -> egui::FontId {
    egui::FontId::new(groesse, egui::FontFamily::Name("Bold".into()))
}

/// Wandelt einen Hex-Farbcode (z. B. `"#1a2b3c"` oder `"1a2b3c"`) in eine egui-Farbe um.
/// Gibt `None` zurück, wenn das Format ungültig ist.
fn hex_farbe_parsen(hex: &str) -> Option<egui::Color32> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

/// Liest die Omarchy-Theme-Farben aus `~/.config/omarchy/current/theme/colors.toml`.
/// Gibt `None` zurück, wenn die Datei fehlt oder nicht lesbar ist.
fn omarchy_farben_laden() -> Option<HashMap<String, egui::Color32>> {
    let home = std::env::var("HOME").ok()?;
    let path = format!("{}/.config/omarchy/current/theme/colors.toml", home);
    let content = std::fs::read_to_string(&path).ok()?;

    let mut colors = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"');
            if let Some(color) = hex_farbe_parsen(value) {
                colors.insert(key, color);
            }
        }
    }
    Some(colors)
}

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("Failed to load icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 750.0])
            .with_app_id("mzprotokoll")
            .with_icon(icon),
        vsync: false,
        ..Default::default()
    };
    eframe::run_native(
        "MZProtokoll",
        options,
        Box::new(|cc| Ok(Box::new(ProtokollApp::new(&cc.egui_ctx)))),
    )
}

// -- Datenmodell --

/// Klassifizierungsstufe eines Protokolls.
/// Steuert, wer das Dokument lesen darf.
#[derive(Clone, Debug, PartialEq)]
enum Sicherheit {
    /// Kein Zugriffsschutz – für alle einsehbar.
    Oeffentlich,
    /// Nur für interne Mitarbeiter gedacht.
    Intern,
    /// Eingeschränkter Empfängerkreis.
    Vertraulich,
    /// Höchste Geheimhaltungsstufe.
    StrengVertraulich,
}

impl Sicherheit {
    /// Gibt den deutschen Anzeigetext der Stufe zurück.
    fn label(&self) -> &str {
        match self {
            Sicherheit::Oeffentlich => "Öffentlich",
            Sicherheit::Intern => "Intern",
            Sicherheit::Vertraulich => "Vertraulich",
            Sicherheit::StrengVertraulich => "Streng vertraulich",
        }
    }

    /// Gibt alle Stufen in der Reihenfolge zurück, wie sie in der UI angezeigt werden.
    fn all() -> &'static [Sicherheit] {
        &[
            Sicherheit::Oeffentlich,
            Sicherheit::Intern,
            Sicherheit::Vertraulich,
            Sicherheit::StrengVertraulich,
        ]
    }
}

/// Typ eines Protokolleintrags – bestimmt Farbe, Beschriftung und
/// welche Felder (Kümmerer, Bis-Datum) im UI und PDF sichtbar sind.
#[derive(Clone, Debug, PartialEq)]
enum Art {
    /// Kein Typ gewählt (leerer Eintrag).
    Leer,
    /// Aufgabe wurde abgebrochen.
    Abgebrochen,
    /// Punkt auf der Tagesordnung.
    Agenda,
    /// Eine getroffene Entscheidung.
    Entscheidung,
    /// Erledigte Aufgabe.
    Fertig,
    /// Idee oder Vorschlag.
    Idee,
    /// Allgemeine Information.
    Info,
    /// Offene Aufgabe mit Kümmerer und Fälligkeitsdatum.
    Todo,
}

impl Art {
    /// Gibt den vollständigen Anzeigetext zurück (für Dropdown und PDF).
    fn label(&self) -> &str {
        match self {
            Art::Leer => "—",
            Art::Abgebrochen => "ABGEBROCHEN",
            Art::Agenda => "AGENDA",
            Art::Entscheidung => "ENTSCHEIDUNG",
            Art::Fertig => "FERTIG",
            Art::Idee => "IDEE",
            Art::Info => "INFO",
            Art::Todo => "TODO",
        }
    }

    /// Gibt den Anzeigetext für das ausgewählte Element im Dropdown zurück.
    /// Bei `Leer` wird ein leerer String zurückgegeben, damit das Feld unaufdringlich wirkt.
    fn selected_label(&self) -> &str {
        match self {
            Art::Leer => "",
            other => other.label(),
        }
    }

    /// Gibt die Hervorhebungsfarbe der Art zurück (für Dropdown-Einträge und Tags).
    fn color(&self) -> egui::Color32 {
        match self {
            Art::Leer => egui::Color32::from_rgb(150, 150, 150),
            Art::Abgebrochen => egui::Color32::from_rgb(231, 76, 60),
            Art::Agenda => egui::Color32::from_rgb(155, 89, 182),
            Art::Entscheidung => egui::Color32::from_rgb(52, 152, 219),
            Art::Fertig => egui::Color32::from_rgb(46, 204, 113),
            Art::Idee => egui::Color32::from_rgb(241, 196, 15),
            Art::Info => egui::Color32::from_rgb(150, 150, 150),
            Art::Todo => egui::Color32::from_rgb(230, 126, 34),
        }
    }

    /// Gibt alle Eintragsarten in der Reihenfolge zurück, wie sie im Dropdown erscheinen.
    fn all() -> &'static [Art] {
        &[
            Art::Leer,
            Art::Abgebrochen,
            Art::Agenda,
            Art::Entscheidung,
            Art::Fertig,
            Art::Idee,
            Art::Info,
            Art::Todo,
        ]
    }
}

/// Eine am Meeting beteiligte Person (Protokollant, Teilnehmer oder zur Kenntnis).
struct Person {
    /// Vollständiger Name der Person.
    name: String,
    /// Kürzel (z. B. „MZ"), das in TODO-Einträgen als Kümmerer verwendet wird.
    kuerzel: String,
    /// `true`, wenn das Kürzel manuell eingegeben wurde und nicht automatisch
    /// aus den Anfangsbuchstaben des Namens abgeleitet werden soll.
    kuerzel_manuell: bool,
}

impl Person {
    /// Erstellt eine leere Person (alle Felder leer).
    fn new() -> Self {
        Self {
            name: String::new(),
            kuerzel: String::new(),
            kuerzel_manuell: false,
        }
    }

    /// Leitet ein Kürzel automatisch aus den Anfangsbuchstaben jedes Namensbestandteils ab.
    /// Beispiel: „Marcel Zimmer" → „MZ".
    fn auto_kuerzel(name: &str) -> String {
        name.split_whitespace()
            .filter_map(|w| w.chars().next())
            .map(|c| c.to_uppercase().to_string())
            .collect()
    }
}

/// Ein einzelner Tabellenzeilen-Eintrag im Protokoll.
struct Eintrag {
    /// Kurzbezeichnung des Eintrags (inaktiv und leer nur bei Art::Todo).
    punkt: String,
    /// Typ des Eintrags (Art::Todo, Art::Info usw.).
    art: Art,
    /// Freitext-Notiz, darf Zeilenumbrüche und Markdown-Links enthalten.
    notiz: String,
    /// Kürzel der verantwortlichen Person (nur bei Art::Todo relevant).
    kuemmerer: String,
    /// Fälligkeitsdatum im Format TT.MM.JJJJ (nur bei Art::Todo relevant).
    bis: String,
}

impl Eintrag {
    /// Erstellt einen leeren Eintrag (Art::Leer, alle Textfelder leer).
    fn new() -> Self {
        Self {
            punkt: String::new(),
            art: Art::Leer,
            notiz: String::new(),
            kuemmerer: String::new(),
            bis: String::new(),
        }
    }
}

/// Farbschema der Anwendungsoberfläche.
#[derive(Clone, Copy, PartialEq)]
enum Theme {
    /// Helles egui-Standard-Theme.
    Hell,
    /// Dunkles Theme mit reinem Schwarz als Hintergrund.
    Dunkel,
    /// Passt Farben automatisch an das aktive Omarchy-Desktop-Theme an.
    Omarchy,
}

impl Theme {
    /// Wechselt zyklisch zum nächsten Theme.
    /// Omarchy wird nur angeboten, wenn die Konfigurationsdatei gefunden wurde.
    fn next(self, has_omarchy: bool) -> Self {
        match self {
            Theme::Hell => Theme::Dunkel,
            Theme::Dunkel => if has_omarchy { Theme::Omarchy } else { Theme::Hell },
            Theme::Omarchy => Theme::Hell,
        }
    }


}

/// Ergebnis eines asynchronen Datei-Dialogs (Laden, Speichern oder PDF-Export).
enum DialogErgebnis {
    /// Eine Markdown-Datei wurde ausgewählt und eingelesen.
    Laden(std::path::PathBuf, String),
    /// Ein Speicherpfad wurde gewählt (Datei wurde bereits geschrieben).
    Speichern(std::path::PathBuf),
    /// Ein PDF-Speicherpfad wurde gewählt.
    PdfExport(std::path::PathBuf),
}

/// Zentraler Anwendungszustand von MZProtokoll.
/// Enthält alle Daten des aktuell geöffneten Protokolls sowie UI-Steuerflags.
struct ProtokollApp {
    // --- Protokoll-Kopfdaten ---
    /// Optionaler Projektname (erscheint klein über dem Titel).
    projekt: String,
    /// Titel / Name des Meetings (Hauptüberschrift).
    titel: String,
    /// Datum als freier Text, z. B. „Montag, 05.02.2026".
    datum_text: String,
    /// Veranstaltungsort des Meetings.
    ort: String,
    /// Person, die das Protokoll führt (Pflichtfeld).
    protokollant: Person,
    /// Liste aller Meetingteilnehmer.
    teilnehmer: Vec<Person>,
    /// Personen, die das Protokoll zur Kenntnis erhalten.
    zur_kenntnis: Vec<Person>,
    /// Freitext-Beschreibung des Meetings.
    ueber_meeting: String,
    /// `true` = Protokoll ist noch ein Entwurf.
    ist_entwurf: bool,
    /// `true` = Protokoll wurde freigegeben.
    ist_freigegeben: bool,
    /// Geheimhaltungsstufe des Protokolls.
    sicherheit: Sicherheit,
    /// Alle Tabelleneinträge des Protokolls.
    eintraege: Vec<Eintrag>,

    // --- UI-Steuerflags ---
    /// Fordert den Fokus für die zuletzt hinzugefügte Teilnehmerzeile an.
    focus_new_teilnehmer: bool,
    /// Fordert den Fokus für die zuletzt hinzugefügte Zur-Kenntnis-Zeile an.
    focus_new_zur_kenntnis: bool,
    /// Aktives Farbschema der UI.
    theme: Theme,
    /// Pfad der aktuell geöffneten/gespeicherten Datei (leer = noch nicht gespeichert).
    save_path: Option<std::path::PathBuf>,
    /// Steuert die Anzeige des Beenden-Bestätigungsdialogs.
    show_quit_dialog: bool,
    /// Steuert die Anzeige des Über-Dialogs.
    show_about_dialog: bool,
    /// Gecachte App-Icon-Textur für den Über-Dialog.
    icon_texture: Option<egui::TextureHandle>,
    /// Steuert die Anzeige des PDF-Fehler-Dialogs (keine Schrift gefunden).
    show_pdf_error: bool,
    /// Steuert die Anzeige des Pflichtfeld-Hinweisdialogs.
    show_pflichtfeld_hinweis: bool,
    /// Index des Notizfeldes, das beim nächsten Frame den Fokus erhalten soll.
    focus_notiz: Option<usize>,
    /// Speichert, welche Notizzeile zuletzt fokussiert war (Index, Cursor-Position).
    /// Wird für die Cursor-Auf/Ab-Navigation zwischen Notizfeldern benötigt.
    notiz_had_focus: Option<(usize, usize)>,
    /// Textfarbe für Eingabefelder im Omarchy-Theme (aus `color2`).
    input_text_color: Option<egui::Color32>,
    /// Farbe für Beschriftungen/Labels im Omarchy-Theme (aus `color3`).
    label_color: Option<egui::Color32>,
    /// `true` wenn eine Omarchy-Theme-Konfiguration gefunden wurde.
    has_omarchy: bool,
    /// Empfangskanal für Ergebnisse aus Datei-Dialog-Threads.
    dialog_rx: Option<mpsc::Receiver<DialogErgebnis>>,
    /// Zwischengespeicherte Schriftfamilie für den PDF-Export (wird nach dem
    /// Dialog-Thread übergeben und dann verbraucht).
    pending_pdf_font: Option<genpdf::fonts::FontFamily<genpdf::fonts::FontData>>,

    // --- Metadaten zur Nachverfolgbarkeit ---
    /// Zeitstempel der Ersterstellung (TT.MM.JJJJ HH:MM), leer wenn noch nicht gespeichert.
    erstellt_am: String,
    /// Name der Person, die das Protokoll erstellt hat.
    erstellt_von: String,
}

impl ProtokollApp {
    /// Initialisiert die App: lädt Systemschriften, ermittelt das aktuelle Datum
    /// und setzt alle Felder auf Standardwerte.
    fn new(ctx: &egui::Context) -> Self {
        // Systemschriften laden: egui benötigt Regular und Bold als separate Font-Families.
        // Liest Schriften zur Laufzeit vom System – keine Schriften werden eingebettet.
        {
            #[cfg(windows)]
            let schrift_paare = [
                ("C:\\Windows\\Fonts\\arial.ttf",    "C:\\Windows\\Fonts\\arialbd.ttf"),
                ("C:\\Windows\\Fonts\\segoeui.ttf",  "C:\\Windows\\Fonts\\segoeuib.ttf"),
                ("C:\\Windows\\Fonts\\calibri.ttf",  "C:\\Windows\\Fonts\\calibrib.ttf"),
                ("C:\\Windows\\Fonts\\tahoma.ttf",   "C:\\Windows\\Fonts\\tahomabd.ttf"),
            ];
            #[cfg(not(windows))]
            let schrift_paare = [
                // Arch, Fedora, openSUSE
                ("/usr/share/fonts/liberation/LiberationSans-Regular.ttf", "/usr/share/fonts/liberation/LiberationSans-Bold.ttf"),
                ("/usr/share/fonts/TTF/LiberationSans-Regular.ttf",        "/usr/share/fonts/TTF/LiberationSans-Bold.ttf"),
                ("/usr/share/fonts/noto/NotoSans-Regular.ttf",             "/usr/share/fonts/noto/NotoSans-Bold.ttf"),
                ("/usr/share/fonts/TTF/NotoSans-Regular.ttf",              "/usr/share/fonts/TTF/NotoSans-Bold.ttf"),
                // Debian, Ubuntu, Mint
                ("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf", "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf"),
                ("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",             "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf"),
                // DejaVu als Fallback
                ("/usr/share/fonts/TTF/DejaVuSans.ttf",                    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf"),
                ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
            ];
            for (regulaer_pfad, fett_pfad) in schrift_paare {
                if let (Ok(regulaer_daten), Ok(fett_daten)) = (std::fs::read(regulaer_pfad), std::fs::read(fett_pfad)) {
                    let mut schriften = egui::FontDefinitions::default();
                    schriften.font_data.insert("regular".to_owned(), egui::FontData::from_owned(regulaer_daten).into());
                    schriften.font_data.insert("bold".to_owned(), egui::FontData::from_owned(fett_daten).into());
                    // Regular als Standard-Proportional-Schrift registrieren
                    if let Some(family) = schriften.families.get_mut(&egui::FontFamily::Proportional) {
                        family.insert(0, "regular".to_owned());
                    }
                    // Bold als eigene Font-Family für Eingabefelder registrieren
                    let mut fette_schriftfamilie = vec!["bold".to_owned()];
                    if let Some(proportional) = schriften.families.get(&egui::FontFamily::Proportional) {
                        fette_schriftfamilie.extend(proportional.iter().cloned());
                    }
                    schriften.families.insert(egui::FontFamily::Name("Bold".into()), fette_schriftfamilie);
                    ctx.set_fonts(schriften);
                    break; // erste gefundene Schrift verwenden
                }
            }
        }

        let heute = Local::now().date_naive();
        let wochentag = match heute.weekday() {
            chrono::Weekday::Mon => "Montag",
            chrono::Weekday::Tue => "Dienstag",
            chrono::Weekday::Wed => "Mittwoch",
            chrono::Weekday::Thu => "Donnerstag",
            chrono::Weekday::Fri => "Freitag",
            chrono::Weekday::Sat => "Samstag",
            chrono::Weekday::Sun => "Sonntag",
        };
        Self {
            projekt: String::new(),
            titel: String::new(),
            datum_text: format!(
                "{}, {:02}.{:02}.{}",
                wochentag,
                heute.day(),
                heute.month(),
                heute.year()
            ),
            ort: String::new(),
            protokollant: Person::new(),
            teilnehmer: vec![Person::new()],
            zur_kenntnis: vec![Person::new()],
            ueber_meeting: String::new(),
            ist_entwurf: true,
            ist_freigegeben: false,
            sicherheit: Sicherheit::Intern,
            eintraege: vec![Eintrag::new()],
            focus_new_teilnehmer: false,
            focus_new_zur_kenntnis: false,
            theme: if omarchy_farben_laden().is_some() { Theme::Omarchy } else { Theme::Dunkel },
            save_path: None,
            show_quit_dialog: false,
            show_about_dialog: false,
            icon_texture: None,
            show_pdf_error: false,
            show_pflichtfeld_hinweis: false,
            focus_notiz: None,
            notiz_had_focus: None,
            input_text_color: None,
            label_color: None,
            has_omarchy: omarchy_farben_laden().is_some(),
            dialog_rx: None,
            pending_pdf_font: None,
            erstellt_am: String::new(),
            erstellt_von: String::new(),
        }
    }

    /// Generiert einen vorgeschlagenen Dateinamen für die Markdown-Datei.
    /// Format: `MZProtokoll_<Titel>__<JJJJ-MM-TT>.md`
    fn dateinamen_erstellen(&self) -> String {
        let name_part: String = self.titel.chars().filter(|c| c.is_alphabetic()).collect();
        let datum = Local::now().format("%Y-%m-%d").to_string();
        format!("MZProtokoll_{}__{}.md", name_part, datum)
    }

    /// Serialisiert den aktuellen Protokollzustand als Markdown-String.
    /// Das Format ist spezifisch für MZProtokoll und wird von `markdown_parsen` wieder eingelesen.
    fn markdown_erstellen(&self) -> String {
        let mut md = String::new();

        if !self.projekt.is_empty() {
            md.push_str(&format!("**Projekt:** {}\n\n", self.projekt));
        }

        md.push_str(&format!("# {}\n\n", self.titel));

        let mut meta = Vec::new();
        if !self.datum_text.is_empty() {
            meta.push(format!("**Datum:** {}", self.datum_text));
        }
        if !self.ort.is_empty() {
            meta.push(format!("**Ort:** {}", self.ort));
        }
        if !meta.is_empty() {
            md.push_str(&meta.join(" | "));
            md.push_str("\n\n");
        }

        md.push_str("---\n\n");

        if !self.protokollant.name.is_empty() {
            md.push_str("## Protokollführer\n\n");
            md.push_str(&self.protokollant.name);
            if !self.protokollant.kuerzel.is_empty() {
                md.push_str(&format!(" [{}]", self.protokollant.kuerzel));
            }
            md.push_str("\n\n");
        }

        let tn: Vec<_> = self.teilnehmer.iter().filter(|t| !t.name.is_empty()).collect();
        if !tn.is_empty() {
            md.push_str("## Teilnehmer\n\n");
            for t in &tn {
                md.push_str(&format!("- {}", t.name));
                if !t.kuerzel.is_empty() {
                    md.push_str(&format!(" [{}]", t.kuerzel));
                }
                md.push('\n');
            }
            md.push('\n');
        }

        let zk: Vec<_> = self.zur_kenntnis.iter().filter(|z| !z.name.is_empty()).collect();
        if !zk.is_empty() {
            md.push_str("## Zur Kenntnis\n\n");
            for z in &zk {
                md.push_str(&format!("- {}", z.name));
                if !z.kuerzel.is_empty() {
                    md.push_str(&format!(" [{}]", z.kuerzel));
                }
                md.push('\n');
            }
            md.push('\n');
        }

        md.push_str("## Über dieses Meeting\n\n");
        if !self.ueber_meeting.is_empty() {
            md.push_str(&self.ueber_meeting);
            md.push_str("\n\n");
        }

        md.push_str("## Status\n\n");
        if self.ist_entwurf {
            md.push_str("- [x] Entwurf\n");
            md.push_str("- [ ] Freigegeben\n");
        } else if self.ist_freigegeben {
            md.push_str("- [ ] Entwurf\n");
            md.push_str("- [x] Freigegeben\n");
        } else {
            md.push_str("- [ ] Entwurf\n");
            md.push_str("- [ ] Freigegeben\n");
        }
        md.push('\n');

        md.push_str("## Klassifizierung\n\n");
        for s in Sicherheit::all() {
            if *s == self.sicherheit {
                md.push_str(&format!("- [x] {}\n", s.label()));
            } else {
                md.push_str(&format!("- [ ] {}\n", s.label()));
            }
        }
        md.push('\n');

        let entries: Vec<_> = self
            .eintraege
            .iter()
            .filter(|e| !e.punkt.is_empty() || e.art != Art::Leer || !e.notiz.is_empty())
            .collect();

        if !entries.is_empty() {
            md.push_str("---\n\n");
            md.push_str("## Einträge\n\n");
            md.push_str("| Punkt | Art | Notiz | Kümmerer | Bis |\n");
            md.push_str("|-------|-----|-------|----------|-----|\n");
            for e in &entries {
                let art_str = if e.art == Art::Leer {
                    ""
                } else {
                    e.art.label()
                };
                let notiz = e.notiz.replace('\n', " <br> ").replace('|', "\\|");
                let punkt = e.punkt.replace('|', "\\|");
                let kuemmerer = e.kuemmerer.replace('|', "\\|");
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    punkt, art_str, notiz, kuemmerer, e.bis
                ));
            }
        }

        md.push_str("\n---\n\n");
        if !self.erstellt_am.is_empty() {
            md.push_str(&format!("**Erstellt:** {} von {}\n\n", self.erstellt_am, self.erstellt_von));
        }
        let geaendert_am = Local::now().format("%d.%m.%Y %H:%M").to_string();
        md.push_str(&format!("**Geändert:** {} von {}\n\n", geaendert_am, self.protokollant.name));
        md.push_str("*Erstellt mit MZProtokoll von Marcel Zimmer — [www.marcelzimmer.de](https://www.marcelzimmer.de) | [X @marcelzimmer](https://x.com/marcelzimmer) | [GitHub @marcelzimmer](https://github.com/marcelzimmer)*\n");

        md
    }

    /// Sortiert Teilnehmer und Zur-Kenntnis-Personen alphabetisch.
    /// Leere Einträge werden ans Ende verschoben.
    fn sort_personen(&mut self) {
        let sort_fn = |a: &Person, b: &Person| {
            let a_empty = a.name.trim().is_empty();
            let b_empty = b.name.trim().is_empty();
            match (a_empty, b_empty) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        };
        self.teilnehmer.sort_by(sort_fn);
        self.zur_kenntnis.sort_by(sort_fn);
    }

    /// Speichert das Protokoll als Markdown-Datei.
    /// Ist bereits ein Pfad bekannt (`save_path`), wird direkt überschrieben.
    /// Andernfalls öffnet sich ein Datei-Speichern-Dialog in einem separaten Thread.
    fn speichern(&mut self) {
        self.sort_personen();
        if self.protokollant.name.trim().is_empty() {
            self.show_pflichtfeld_hinweis = true;
            return;
        }
        if self.erstellt_am.is_empty() {
            self.erstellt_am = Local::now().format("%d.%m.%Y %H:%M").to_string();
            self.erstellt_von = self.protokollant.name.clone();
        }
        let content = self.markdown_erstellen();

        if let Some(ref path) = self.save_path {
            let _ = std::fs::write(path, content);
        } else {
            let filename = self.dateinamen_erstellen();
            let (tx, rx) = mpsc::channel();
            self.dialog_rx = Some(rx);
            std::thread::spawn(move || {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(&filename)
                    .add_filter("Markdown", &["md"])
                    .save_file()
                {
                    let _ = std::fs::write(&path, &content);
                    let _ = tx.send(DialogErgebnis::Speichern(path));
                }
            });
        }
    }

    /// Öffnet einen Datei-Öffnen-Dialog (separater Thread) und lädt
    /// die gewählte Markdown-Datei via `markdown_parsen` in den App-Zustand.
    fn laden(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.dialog_rx = Some(rx);
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Markdown", &["md"])
                .pick_file()
            {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let _ = tx.send(DialogErgebnis::Laden(path, content));
                }
            }
        });
    }

    /// Liest einen MZProtokoll-Markdown-String ein und befüllt alle Felder
    /// der App. Vorhandene Daten werden dabei vollständig überschrieben.
    /// Der Parser ist zeilenbasiert und arbeitet mit einem Sektions-Zustandsautomaten.
    fn markdown_parsen(&mut self, content: &str) {
        self.projekt = String::new();
        self.titel = String::new();
        self.datum_text = String::new();
        self.ort = String::new();
        self.protokollant = Person::new();
        self.teilnehmer.clear();
        self.zur_kenntnis.clear();
        self.ueber_meeting = String::new();
        self.ist_entwurf = true;
        self.ist_freigegeben = false;
        self.sicherheit = Sicherheit::Intern;
        self.eintraege.clear();
        self.erstellt_am = String::new();
        self.erstellt_von = String::new();

        #[derive(PartialEq)]
        enum Section {
            Header,
            Protokollfuehrer,
            Teilnehmer,
            ZurKenntnis,
            UeberMeeting,
            Status,
            Sicherheit,
            Eintraege,
        }

        let mut section = Section::Header;
        let mut table_rows_seen = 0u32;
        let mut ueber_lines: Vec<&str> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Erstellt-Metadaten parsen (stehen am Ende der Datei)
            if trimmed.starts_with("**Erstellt:**") {
                let rest = trimmed.trim_start_matches("**Erstellt:**").trim();
                if let Some((datum, von)) = rest.split_once(" von ") {
                    self.erstellt_am = datum.trim().to_string();
                    self.erstellt_von = von.trim().to_string();
                }
                continue;
            }

            // Sektionswechsel bei ## Überschriften
            if trimmed.starts_with("## ") {
                if section == Section::UeberMeeting {
                    self.ueber_meeting = ueber_lines.join("\n").trim().to_string();
                    ueber_lines.clear();
                }

                if trimmed.starts_with("## Protokollführer") {
                    section = Section::Protokollfuehrer;
                    continue;
                } else if trimmed.starts_with("## Teilnehmer") {
                    section = Section::Teilnehmer;
                    continue;
                } else if trimmed.starts_with("## Zur Kenntnis") {
                    section = Section::ZurKenntnis;
                    continue;
                } else if trimmed.starts_with("## Über dieses Meeting") {
                    section = Section::UeberMeeting;
                    continue;
                } else if trimmed.starts_with("## Status") {
                    section = Section::Status;
                    continue;
                } else if trimmed.starts_with("## Klassifizierung") {
                    section = Section::Sicherheit;
                    continue;
                } else if trimmed.starts_with("## Einträge") {
                    section = Section::Eintraege;
                    table_rows_seen = 0;
                    continue;
                }
            }

            match section {
                Section::Header => {
                    if trimmed.starts_with("**Projekt:**") {
                        self.projekt =
                            trimmed.trim_start_matches("**Projekt:**").trim().to_string();
                    } else if trimmed.starts_with("# ") {
                        self.titel = trimmed[2..].to_string();
                    } else if trimmed.contains("**Datum:**") || trimmed.contains("**Ort:**") {
                        for part in trimmed.split(" | ") {
                            let part = part.trim();
                            if part.starts_with("**Datum:**") {
                                self.datum_text =
                                    part.trim_start_matches("**Datum:**").trim().to_string();
                            } else if part.starts_with("**Ort:**") {
                                self.ort = part.trim_start_matches("**Ort:**").trim().to_string();
                            }
                        }
                    }
                }
                Section::Protokollfuehrer => {
                    if !trimmed.is_empty() && trimmed != "---" {
                        let (name, kuerzel) = name_kuerzel_parsen(trimmed);
                        self.protokollant.name = name;
                        if !kuerzel.is_empty() {
                            self.protokollant.kuerzel = kuerzel;
                            self.protokollant.kuerzel_manuell = true;
                        }
                    }
                }
                Section::Teilnehmer => {
                    if trimmed.starts_with("- ") {
                        let (name, kuerzel) = name_kuerzel_parsen(&trimmed[2..]);
                        let mut p = Person::new();
                        p.name = name;
                        if !kuerzel.is_empty() {
                            p.kuerzel = kuerzel;
                            p.kuerzel_manuell = true;
                        }
                        self.teilnehmer.push(p);
                    }
                }
                Section::ZurKenntnis => {
                    if trimmed.starts_with("- ") {
                        let (name, kuerzel) = name_kuerzel_parsen(&trimmed[2..]);
                        let mut p = Person::new();
                        p.name = name;
                        if !kuerzel.is_empty() {
                            p.kuerzel = kuerzel;
                            p.kuerzel_manuell = true;
                        }
                        self.zur_kenntnis.push(p);
                    }
                }
                Section::UeberMeeting => {
                    if trimmed != "---" {
                        ueber_lines.push(line);
                    }
                }
                Section::Status => {
                    if trimmed.starts_with("- [x] Entwurf") {
                        self.ist_entwurf = true;
                    } else if trimmed.starts_with("- [x] Freigegeben") {
                        self.ist_freigegeben = true;
                    }
                }
                Section::Sicherheit => {
                    if trimmed.starts_with("- [x] Öffentlich") {
                        self.sicherheit = Sicherheit::Oeffentlich;
                    } else if trimmed.starts_with("- [x] Intern") {
                        self.sicherheit = Sicherheit::Intern;
                    } else if trimmed.starts_with("- [x] Vertraulich") {
                        self.sicherheit = Sicherheit::Vertraulich;
                    } else if trimmed.starts_with("- [x] Streng vertraulich") {
                        self.sicherheit = Sicherheit::StrengVertraulich;
                    }
                }
                Section::Eintraege => {
                    if trimmed.starts_with('|') {
                        table_rows_seen += 1;
                        // Zeile 1 = Kopfzeile, Zeile 2 = Trennlinie, ab Zeile 3 = Daten
                        if table_rows_seen >= 3 {
                            let cells = tabellenzeile_aufteilen(trimmed);
                            if cells.len() >= 5 {
                                let mut e = Eintrag::new();
                                e.punkt = cells[0].clone();
                                e.art = art_parsen(&cells[1]);
                                e.notiz = cells[2].replace(" <br> ", "\n");
                                e.kuemmerer = cells[3].clone();
                                e.bis = cells[4].clone();
                                if e.art == Art::Todo {
                                    e.punkt.clear();
                                }
                                self.eintraege.push(e);
                            }
                        }
                    }
                }
            }
        }

        // Restlichen "Über dieses Meeting"-Text flushen
        if section == Section::UeberMeeting {
            self.ueber_meeting = ueber_lines.join("\n").trim().to_string();
        }

        // Mindestens je einen leeren Eintrag sicherstellen
        if self.teilnehmer.is_empty() {
            self.teilnehmer.push(Person::new());
        }
        if self.zur_kenntnis.is_empty() {
            self.zur_kenntnis.push(Person::new());
        }
        if self.eintraege.is_empty() {
            self.eintraege.push(Eintrag::new());
        }
    }

    /// Generiert einen vorgeschlagenen Dateinamen für den PDF-Export.
    /// Format: `MZProtokoll_<Titel>__<JJJJ-MM-TT>.pdf`
    fn pdf_dateinamen_erstellen(&self) -> String {
        let name_part: String = self.titel.chars().filter(|c| c.is_alphabetic()).collect();
        let datum = Local::now().format("%Y-%m-%d").to_string();
        format!("MZProtokoll_{}__{}.pdf", name_part, datum)
    }

    /// Sucht auf dem System nach einer passenden Schriftfamilie für den PDF-Export.
    /// Probiert nacheinander Liberation Sans, Noto Sans und DejaVu Sans.
    /// Gibt `None` zurück, wenn keine Schrift gefunden wird.
    fn schrift_laden(&self) -> Option<genpdf::fonts::FontFamily<genpdf::fonts::FontData>> {
        // Liest Schriften zur Laufzeit vom System – keine Schriften werden eingebettet.

        // 1. Linux: Schriftfamilien mit Standard-Benennung (Name-Regular.ttf, Name-Bold.ttf, ...)
        #[cfg(not(windows))]
        {
            let schrift_familien = [
                ("/usr/share/fonts/liberation",          "LiberationSans"),
                ("/usr/share/fonts/noto",                "NotoSans"),
                ("/usr/share/fonts/TTF",                 "LiberationSans"),
                ("/usr/share/fonts/TTF",                 "NotoSans"),
                ("/usr/share/fonts/truetype/liberation", "LiberationSans"),
                ("/usr/share/fonts/truetype/noto",       "NotoSans"),
            ];
            for (pfad, familie) in schrift_familien {
                if let Ok(schrift) = genpdf::fonts::from_files(pfad, familie, None) {
                    return Some(schrift);
                }
            }
        }

        // 2. Einzelne .ttf-Dateien (Windows-Systemschriften + Linux DejaVu als Fallback)
        #[cfg(windows)]
        let einzel_schriften = [
            ("C:\\Windows\\Fonts\\arial.ttf",   "C:\\Windows\\Fonts\\arialbd.ttf"),
            ("C:\\Windows\\Fonts\\verdana.ttf", "C:\\Windows\\Fonts\\verdanab.ttf"),
            ("C:\\Windows\\Fonts\\calibri.ttf", "C:\\Windows\\Fonts\\calibrib.ttf"),
            ("C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\segoeuib.ttf"),
        ];
        #[cfg(not(windows))]
        let einzel_schriften = [
            ("/usr/share/fonts/TTF/DejaVuSans.ttf",                    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf"),
            ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
            ("/usr/share/fonts/TTF/DejaVuSans.ttf",                    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf"),
            ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        ];
        for (regular_path, bold_path) in einzel_schriften {
            if let Ok(regular_data) = std::fs::read(regular_path) {
                let bold_data = std::fs::read(bold_path).unwrap_or_else(|_| regular_data.clone());
                if let (Ok(regular), Ok(bold), Ok(italic), Ok(bold_italic)) = (
                    genpdf::fonts::FontData::new(regular_data.clone(), None),
                    genpdf::fonts::FontData::new(bold_data.clone(), None),
                    genpdf::fonts::FontData::new(regular_data, None),
                    genpdf::fonts::FontData::new(bold_data, None),
                ) {
                    return Some(genpdf::fonts::FontFamily { regular, bold, italic, bold_italic });
                }
            }
        }
        None
    }

    /// Fügt den gesamten Protokollinhalt (Kopfdaten, Eintrags-Tabelle, Links)
    /// in das übergebene genpdf-Dokument ein.
    /// Wird zweimal aufgerufen: einmal für den Vorberechnungsdurchlauf
    /// (Seitenanzahl ermitteln) und einmal für den eigentlichen Export.
    fn pdf_inhalt_hinzufuegen(&self, doc: &mut genpdf::Document) {
        let small = genpdf::style::Style::new().with_font_size(9);
        let small_bold = genpdf::style::Style::new().bold().with_font_size(9);
        let heading_style = genpdf::style::Style::new().bold().with_font_size(20);

        // Projekt
        if !self.projekt.is_empty() {
            doc.push(
                genpdf::elements::Paragraph::new(&self.projekt)
                    .styled(small),
            );
        }

        // Titel
        doc.push(
            genpdf::elements::Paragraph::new(&self.titel)
                .styled(heading_style),
        );
        doc.push(genpdf::elements::Break::new(0.5));

        // Datum | Ort
        let mut meta_parts = Vec::new();
        if !self.datum_text.is_empty() {
            meta_parts.push(format!("Datum: {}", self.datum_text));
        }
        if !self.ort.is_empty() {
            meta_parts.push(format!("Ort: {}", self.ort));
        }
        if !meta_parts.is_empty() {
            doc.push(genpdf::elements::Paragraph::new(meta_parts.join("  |  ")).styled(small));
            doc.push(genpdf::elements::Break::new(0.5));
        }

        // Trennlinie
        doc.push(
            genpdf::elements::Paragraph::new("_".repeat(250))
                .styled(genpdf::style::Style::new().with_font_size(6).with_color(
                    genpdf::style::Color::Greyscale(180),
                )),
        );
        doc.push(genpdf::elements::Break::new(0.5));

        // Protokollführer, Teilnehmer, Zur Kenntnis, Über dieses Meeting
        // als zweispaltige Tabelle, damit die Werte bündig starten
        {
            let mut info_table = genpdf::elements::TableLayout::new(vec![3, 11]);

            // Protokollführer
            if !self.protokollant.name.is_empty() {
                let mut name = self.protokollant.name.clone();
                if !self.protokollant.kuerzel.is_empty() {
                    name.push_str(&format!(" [{}]", self.protokollant.kuerzel));
                }
                let _ = info_table.row()
                    .element(genpdf::elements::Paragraph::new("Protokollführer").styled(small_bold).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .element(genpdf::elements::Paragraph::new(name).styled(small).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .push();
            }

            // Teilnehmer
            let tn: Vec<_> = self.teilnehmer.iter().filter(|t| !t.name.is_empty()).collect();
            if !tn.is_empty() {
                let namen: Vec<String> = tn.iter().map(|t| {
                    let mut text = t.name.clone();
                    if !t.kuerzel.is_empty() {
                        text.push_str(&format!(" [{}]", t.kuerzel));
                    }
                    text
                }).collect();
                let _ = info_table.row()
                    .element(genpdf::elements::Paragraph::new("Teilnehmer").styled(small_bold).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .element(genpdf::elements::Paragraph::new(namen.join(", ")).styled(small).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .push();
            }

            // Zur Kenntnis
            let zk: Vec<_> = self.zur_kenntnis.iter().filter(|z| !z.name.is_empty()).collect();
            if !zk.is_empty() {
                let namen: Vec<String> = zk.iter().map(|z| {
                    let mut text = z.name.clone();
                    if !z.kuerzel.is_empty() {
                        text.push_str(&format!(" [{}]", z.kuerzel));
                    }
                    text
                }).collect();
                let _ = info_table.row()
                    .element(genpdf::elements::Paragraph::new("Zur Kenntnis").styled(small_bold).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .element(genpdf::elements::Paragraph::new(namen.join(", ")).styled(small).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .push();
            }

            // Über dieses Meeting
            if !self.ueber_meeting.is_empty() {
                let _ = info_table.row()
                    .element(genpdf::elements::Paragraph::new("Über dieses Meeting").styled(small_bold).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .element(genpdf::elements::Paragraph::new(&self.ueber_meeting).styled(small).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .push();
            }

            // Status (Entwurf / Freigegeben)
            {
                let entwurf = if self.ist_entwurf { "[x] Entwurf" } else { "[  ] Entwurf" };
                let freigegeben = if self.ist_freigegeben { "[x] Freigegeben" } else { "[  ] Freigegeben" };
                let mut cb_table = genpdf::elements::TableLayout::new(vec![1, 1, 1, 1]);
                let _ = cb_table.row()
                    .element(genpdf::elements::Paragraph::new(entwurf).styled(small))
                    .element(genpdf::elements::Paragraph::new(freigegeben).styled(small))
                    .element(genpdf::elements::Paragraph::new(""))
                    .element(genpdf::elements::Paragraph::new(""))
                    .push();
                let _ = info_table.row()
                    .element(genpdf::elements::Paragraph::new("Status").styled(small_bold).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .element(cb_table.padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .push();
            }

            // Klassifizierung
            {
                let entries: Vec<String> = Sicherheit::all()
                    .iter()
                    .map(|s| {
                        if *s == self.sicherheit {
                            format!("[x] {}", s.label())
                        } else {
                            format!("[  ] {}", s.label())
                        }
                    })
                    .collect();
                let mut cb_table = genpdf::elements::TableLayout::new(vec![1, 1, 1, 1]);
                let _ = cb_table.row()
                    .element(genpdf::elements::Paragraph::new(entries[0].clone()).styled(small))
                    .element(genpdf::elements::Paragraph::new(entries[1].clone()).styled(small))
                    .element(genpdf::elements::Paragraph::new(entries[2].clone()).styled(small))
                    .element(genpdf::elements::Paragraph::new(entries[3].clone()).styled(small))
                    .push();
                let _ = info_table.row()
                    .element(genpdf::elements::Paragraph::new("Klassifizierung").styled(small_bold).padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .element(cb_table.padded(genpdf::Margins::trbl(1, 0, 1, 0)))
                    .push();
            }

            doc.push(info_table);
            doc.push(genpdf::elements::Break::new(0.5));
        }

        // Trennlinie
        doc.push(
            genpdf::elements::Paragraph::new("_".repeat(250))
                .styled(genpdf::style::Style::new().with_font_size(6).with_color(
                    genpdf::style::Color::Greyscale(180),
                )),
        );
        doc.push(genpdf::elements::Break::new(0.5));

        // Einträge als Tabelle
        let entries: Vec<_> = self
            .eintraege
            .iter()
            .filter(|e| !e.punkt.is_empty() || e.art != Art::Leer || !e.notiz.is_empty())
            .collect();

        if !entries.is_empty() {
            let mut all_links: Vec<(usize, String, String)> = Vec::new();
            let mut table = genpdf::elements::TableLayout::new(vec![3, 5, 13, 4, 4]);

            // Kopfzeile
            let _ = table
                .row()
                .element(
                    genpdf::elements::Paragraph::new("")
                        .styled(small_bold)
                        .padded(genpdf::Margins::trbl(1, 2, 1, 0)),
                )
                .element(
                    genpdf::elements::Paragraph::new("Art")
                        .styled(small_bold)
                        .padded(genpdf::Margins::trbl(1, 2, 1, 2)),
                )
                .element(
                    genpdf::elements::Paragraph::new("Notiz")
                        .styled(small_bold)
                        .padded(genpdf::Margins::trbl(1, 2, 1, 2)),
                )
                .element(
                    genpdf::elements::Paragraph::new("Kümmerer")
                        .styled(small_bold)
                        .padded(genpdf::Margins::trbl(1, 2, 1, 2)),
                )
                .element(
                    genpdf::elements::Paragraph::new("Bis")
                        .styled(small_bold)
                        .padded(genpdf::Margins::trbl(1, 2, 1, 2)),
                )
                .push();

            for e in &entries {
                let art_str = if e.art == Art::Leer {
                    ""
                } else {
                    e.art.label()
                };
                let is_todo = e.art == Art::Todo;
                let row_style = if is_todo { small_bold } else { small };

                let notiz_cell = {
                    let mut layout = genpdf::elements::LinearLayout::vertical();
                    for line in e.notiz.split('\n') {
                        let (replaced, new_links) =
                            markdown_links_extrahieren(line, all_links.len() + 1);
                        all_links.extend(new_links);
                        layout.push(
                            genpdf::elements::Paragraph::new(replaced)
                                .styled(row_style),
                        );
                    }
                    layout.padded(genpdf::Margins::trbl(1, 2, 1, 2))
                };

                if is_todo {
                    // Großzügiger max_height — nächste Zeile mit weißem Hintergrund deckt Überlauf ab
                    let notiz_lines = e.notiz.split('\n').count().max(1) as f64;
                    let row_h = notiz_lines * 8.0 + 10.0;

                    let _ = table
                        .row()
                        .element(ZellenHintergrund::grau(
                            genpdf::elements::Paragraph::new(&e.punkt)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.5, 2, 2.5, 0)),
                            row_h,
                        ))
                        .element(ZellenHintergrund::grau(
                            genpdf::elements::Paragraph::new(art_str)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.5, 2, 2.5, 2)),
                            row_h,
                        ))
                        .element(ZellenHintergrund::grau(
                            notiz_cell.padded(genpdf::Margins::trbl(0.5, 0, 1.5, 0)),
                            row_h,
                        ))
                        .element(ZellenHintergrund::grau(
                            genpdf::elements::Paragraph::new(&e.kuemmerer)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.5, 2, 2.5, 2)),
                            row_h,
                        ))
                        .element(ZellenHintergrund::grau(
                            genpdf::elements::Paragraph::new(&e.bis)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.5, 2, 2.5, 2)),
                            row_h,
                        ))
                        .push();
                } else {
                    // Weißer Hintergrund deckt etwaigen Grau-Überlauf der Zeile darüber ab
                    let white_h = 40.0;
                    let _ = table
                        .row()
                        .element(ZellenHintergrund::weiss(
                            genpdf::elements::Paragraph::new(&e.punkt)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.75, 2, 2.25, 0)),
                            white_h,
                        ))
                        .element(ZellenHintergrund::weiss(
                            genpdf::elements::Paragraph::new(art_str)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.75, 2, 2.25, 2)),
                            white_h,
                        ))
                        .element(ZellenHintergrund::weiss(
                            notiz_cell.padded(genpdf::Margins::trbl(0.75, 0, 1.25, 0)),
                            white_h,
                        ))
                        .element(ZellenHintergrund::weiss(
                            genpdf::elements::Paragraph::new(&e.kuemmerer)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.75, 2, 2.25, 2)),
                            white_h,
                        ))
                        .element(ZellenHintergrund::weiss(
                            genpdf::elements::Paragraph::new(&e.bis)
                                .styled(row_style)
                                .padded(genpdf::Margins::trbl(1.75, 2, 2.25, 2)),
                            white_h,
                        ))
                        .push();
                }
            }

            doc.push(table);

            if !all_links.is_empty() {
                let tiny = genpdf::style::Style::new().with_font_size(7);
                let tiny_bold = genpdf::style::Style::new().bold().with_font_size(9);
                doc.push(genpdf::elements::Break::new(1.0));
                doc.push(
                    genpdf::elements::Paragraph::new("Links")
                        .styled(tiny_bold),
                );
                doc.push(genpdf::elements::Break::new(0.3));
                for (num, label, url) in &all_links {
                    let mut layout = genpdf::elements::LinearLayout::vertical();
                    layout.push(
                        genpdf::elements::Paragraph::new(
                            format!("[{}] {}:", num, label),
                        )
                        .styled(tiny),
                    );
                    // URL an '/' aufteilen, damit genpdf umbrechen kann
                    let mut url_lines: Vec<String> = Vec::new();
                    let mut current = String::new();
                    for ch in url.chars() {
                        current.push(ch);
                        if ch == '/' && current.len() > 100 {
                            url_lines.push(current);
                            current = String::new();
                        }
                    }
                    if !current.is_empty() {
                        url_lines.push(current);
                    }
                    for chunk in &url_lines {
                        layout.push(
                            genpdf::elements::Paragraph::new(chunk.as_str())
                                .styled(tiny)
                                .padded(genpdf::Margins::trbl(0, 0, 0, 3.5)),
                        );
                    }
                    doc.push(layout);
                }
            }
        }
    }

    /// Startet den PDF-Export-Prozess:
    /// 1. Personen sortieren und Pflichtfelder prüfen.
    /// 2. Markdown automatisch speichern (falls Pfad bekannt).
    /// 3. Schriftart laden (Fehler → Fehlerdialog).
    /// 4. Datei-Speichern-Dialog in separatem Thread öffnen.
    /// 5. Bei Bestätigung: `pdf_generieren` aufrufen.
    fn pdf_exportieren(&mut self) {
        self.sort_personen();
        if self.protokollant.name.trim().is_empty() {
            self.show_pflichtfeld_hinweis = true;
            return;
        }
        // Vor PDF-Erzeugung automatisch speichern
        if let Some(ref path) = self.save_path {
            if self.erstellt_am.is_empty() {
                self.erstellt_am = Local::now().format("%d.%m.%Y %H:%M").to_string();
                self.erstellt_von = self.protokollant.name.clone();
            }
            let content = self.markdown_erstellen();
            let _ = std::fs::write(path, content);
        }
        let font_family = match self.schrift_laden() {
            Some(f) => f,
            None => {
                self.show_pdf_error = true;
                return;
            }
        };

        self.pending_pdf_font = Some(font_family);
        let pdf_filename = self.pdf_dateinamen_erstellen();
        let (tx, rx) = mpsc::channel();
        self.dialog_rx = Some(rx);
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&pdf_filename)
                .add_filter("PDF", &["pdf"])
                .save_file()
            {
                let _ = tx.send(DialogErgebnis::PdfExport(path));
            }
        });
    }

    /// Rendert das Protokoll als PDF-Datei in zwei Durchläufen:
    /// - **Durchlauf 1**: Inhalt in einen In-Memory-Puffer rendern, um die Gesamtseitenzahl
    ///   zu ermitteln (genpdf kennt diese erst nach dem Rendern).
    /// - **Durchlauf 2**: Inhalt erneut rendern, diesmal mit `FusszeileDekorator`, der
    ///   die korrekte Gesamtseitenzahl in die Fußzeile schreibt.
    fn pdf_generieren(&self, path: &std::path::Path, schriftfamilie: genpdf::fonts::FontFamily<genpdf::fonts::FontData>) {
        // Durchlauf 1: Gesamtseitenzahl durch In-Memory-Rendering ermitteln
        let gesamtseiten = {
            let seitenanzahl = std::rc::Rc::new(std::cell::Cell::new(0usize));
            let zaehler = seitenanzahl.clone();

            let mut vorberechnungs_dok = genpdf::Document::new(schriftfamilie.clone());
            let mut dekorator = genpdf::SimplePageDecorator::new();
            dekorator.set_margins(20);
            // Callback wird pro Seite aufgerufen – speichert die letzte Seitennummer
            dekorator.set_header(move |seite| {
                zaehler.set(seite);
                genpdf::elements::Break::new(0.0)
            });
            vorberechnungs_dok.set_page_decorator(dekorator);
            self.pdf_inhalt_hinzufuegen(&mut vorberechnungs_dok);
            let mut puffer = Vec::new();
            let _ = vorberechnungs_dok.render(&mut puffer);
            seitenanzahl.get()
        };

        // Durchlauf 2: Echtes PDF mit Fußzeile und korrekter Gesamtseitenzahl erstellen
        let mut dok = genpdf::Document::new(schriftfamilie);
        let pdf_titel = if self.titel.is_empty() {
            "MZProtokoll".to_string()
        } else {
            format!("{} — MZProtokoll von Marcel Zimmer (www.marcelzimmer.de)", self.titel)
        };
        dok.set_title(&pdf_titel);
        dok.set_page_decorator(FusszeileDekorator::new(gesamtseiten));
        self.pdf_inhalt_hinzufuegen(&mut dok);
        let _ = dok.render_to_file(path);
    }

    /// Gibt alle bekannten Kürzel (Protokollant + Teilnehmer + Zur-Kenntnis)
    /// sortiert und dedupliziert zurück. Wird für das Kümmerer-Dropdown in TODO-Zeilen verwendet.
    fn alle_kuerzel(&self) -> Vec<String> {
        let mut k = Vec::new();
        if !self.protokollant.kuerzel.is_empty() {
            k.push(self.protokollant.kuerzel.clone());
        }
        for t in &self.teilnehmer {
            if !t.kuerzel.is_empty() {
                k.push(t.kuerzel.clone());
            }
        }
        for z in &self.zur_kenntnis {
            if !z.kuerzel.is_empty() {
                k.push(z.kuerzel.clone());
            }
        }
        k.sort();
        k.dedup();
        k
    }
}

// -- Parse-Helfer --

/// Trennt einen Personeneintrag der Form `"Name [Kürzel]"` in Name und Kürzel auf.
/// Wenn kein Kürzel in eckigen Klammern vorhanden ist, wird ein leerer Kürzel-String zurückgegeben.
fn name_kuerzel_parsen(s: &str) -> (String, String) {
    let trimmed = s.trim();
    if let Some(bracket_start) = trimmed.rfind('[') {
        if let Some(bracket_end) = trimmed.rfind(']') {
            if bracket_end > bracket_start {
                let name = trimmed[..bracket_start].trim().to_string();
                let kuerzel = trimmed[bracket_start + 1..bracket_end].trim().to_string();
                return (name, kuerzel);
            }
        }
    }
    (trimmed.to_string(), String::new())
}

/// Wandelt den Text einer Markdown-Tabellenzelle in die zugehörige `Art`-Variante um.
/// Unbekannte Strings werden als `Art::Leer` interpretiert.
fn art_parsen(s: &str) -> Art {
    match s.trim() {
        "ABGEBROCHEN" => Art::Abgebrochen,
        "AGENDA" => Art::Agenda,
        "ENTSCHEIDUNG" => Art::Entscheidung,
        "FERTIG" => Art::Fertig,
        "IDEE" => Art::Idee,
        "INFO" => Art::Info,
        "TODO" => Art::Todo,
        _ => Art::Leer,
    }
}

/// Teilt eine Markdown-Tabellenzeile (`| A | B | C |`) in einzelne Zellen auf.
/// Berücksichtigt escaped Pipe-Zeichen (`\|`), die innerhalb von Zellen vorkommen dürfen.
fn tabellenzeile_aufteilen(row: &str) -> Vec<String> {
    let trimmed = row.trim().trim_start_matches('|').trim_end_matches('|');
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = trimmed.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '|' {
                    current.push('|');
                    chars.next();
                    continue;
                }
            }
            current.push(c);
        } else if c == '|' {
            cells.push(current.trim().to_string());
            current = String::new();
        } else {
            current.push(c);
        }
    }
    cells.push(current.trim().to_string());
    cells
}

/// Ersetzt Markdown-Links der Form `[Text](URL)` durch `Text [N]` und
/// gibt eine Liste der gefundenen Links als Tupel `(Nummer, Text, URL)` zurück.
/// `start_num` gibt die erste Fußnotennummer an (1-basiert).
/// Wird für den PDF-Export verwendet, da genpdf keine Hyperlinks unterstützt.
fn markdown_links_extrahieren(text: &str, start_num: usize) -> (String, Vec<(usize, String, String)>) {
    let mut result = String::new();
    let mut links: Vec<(usize, String, String)> = Vec::new();
    let mut num = start_num;
    let mut pos = 0;

    while pos < text.len() {
        if text.as_bytes()[pos] == b'[' {
            let after_bracket = pos + 1;
            if after_bracket < text.len() {
                if let Some(rel_close) = text[after_bracket..].find(']') {
                    let close_bracket = after_bracket + rel_close;
                    let label = &text[after_bracket..close_bracket];
                    let after_close = close_bracket + 1;
                    if after_close < text.len() && text.as_bytes()[after_close] == b'(' {
                        let after_paren = after_close + 1;
                        if after_paren < text.len() {
                            if let Some(rel_end) = text[after_paren..].find(')') {
                                let close_paren = after_paren + rel_end;
                                let url = &text[after_paren..close_paren];
                                if !label.is_empty() && !url.is_empty() {
                                    result.push_str(&format!("{} [{}]", label, num));
                                    links.push((num, label.to_string(), url.to_string()));
                                    num += 1;
                                    pos = close_paren + 1;
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
            result.push('[');
            pos += 1;
        } else {
            let ch = text[pos..].chars().next().unwrap();
            result.push(ch);
            pos += ch.len_utf8();
        }
    }

    (result, links)
}

// -- PDF-Helfer --

/// Seitendekorierer für den PDF-Export: fügt jeder Seite eine Fußzeile
/// mit der aktuellen Seitenzahl und der Gesamtseitenanzahl hinzu.
struct FusszeileDekorator {
    /// Seitenränder für den Inhaltsbereich (oben, rechts, unten, links in mm).
    raender: genpdf::Margins,
    /// Laufende Seitennummer (wird beim Rendern pro Seite hochgezählt).
    aktuelle_seite: usize,
    /// Gesamtanzahl der Seiten (aus dem ersten Render-Durchlauf).
    gesamtseiten: usize,
}

impl FusszeileDekorator {
    /// Erstellt einen neuen Fußzeile-Dekorierer mit der bekannten Gesamtseitenzahl.
    fn new(gesamtseiten: usize) -> Self {
        Self {
            raender: genpdf::Margins::trbl(20, 15, 20, 15),
            aktuelle_seite: 0,
            gesamtseiten,
        }
    }
}

impl genpdf::PageDecorator for FusszeileDekorator {
    fn decorate_page<'a>(
        &mut self,
        context: &genpdf::Context,
        area: genpdf::render::Area<'a>,
        _style: genpdf::style::Style,
    ) -> Result<genpdf::render::Area<'a>, genpdf::error::Error> {
        self.aktuelle_seite += 1;

        let mut area = area;

        // Fußzeile auf der Rohseite platzieren, bevor die Seitenränder gesetzt werden
        let rohseiten_groesse = area.size();
        let rohseite_hoehe: f64 = rohseiten_groesse.height.into();
        let rohseite_breite: f64 = rohseiten_groesse.width.into();

        let fusszeilen_text = format!(
            "Seite {} von {}",
            self.aktuelle_seite, self.gesamtseiten
        );
        let fusszeilen_stil = genpdf::style::Style::new().with_font_size(9);
        // Textbreite bei 9pt: ca. 2.0 mm pro Zeichen (Näherungswert)
        let text_breite = fusszeilen_text.len() as f64 * 2.0;
        // Text bündig mit dem rechten Inhaltsrand ausrichten
        let rechter_rand = 8.0;

        let _ = area.print_str(
            &context.font_cache,
            genpdf::Position::new(rohseite_breite - rechter_rand - text_breite, rohseite_hoehe - 15.0),
            fusszeilen_stil,
            &fusszeilen_text,
        );

        // Seitenränder für den eigentlichen Inhaltsbereich anwenden
        area.add_margins(self.raender);

        Ok(area)
    }
}

/// Wrapper-Element für genpdf: zeichnet zuerst einen farbigen Hintergrund (durch
/// dichte horizontale Linien simuliert), danach wird der eigentliche Inhalt darüber gerendert.
struct ZellenHintergrund<E: genpdf::Element> {
    /// Das eingebettete genpdf-Element, das nach dem Hintergrund gerendert wird.
    inhalt: E,
    /// Hintergrundfarbe (Graustufe oder RGB).
    farbe: genpdf::style::Color,
    /// Zusätzlicher Überhang nach links in mm (aktuell immer 0).
    erweiterung_links: f64,
    /// Maximale Hintergrundhöhe in mm — verhindert, dass grauer Überlauf
    /// auf die nächste weiße Zeile reicht.
    max_hoehe: f64,
}

impl<E: genpdf::Element> ZellenHintergrund<E> {
    /// Erstellt eine graue Hintergrundzeile (Graustufe 220).
    fn grau(inhalt: E, max_hoehe: f64) -> Self {
        Self {
            inhalt,
            farbe: genpdf::style::Color::Greyscale(220),
            erweiterung_links: 0.0,
            max_hoehe,
        }
    }
    /// Erstellt eine weiße Hintergrundzeile (deckt grauen Überlauf der Vorgängerzeile ab).
    fn weiss(inhalt: E, max_hoehe: f64) -> Self {
        Self {
            inhalt,
            farbe: genpdf::style::Color::Greyscale(255),
            erweiterung_links: 0.0,
            max_hoehe,
        }
    }
}

impl<E: genpdf::Element> genpdf::Element for ZellenHintergrund<E> {
    fn render(
        &mut self,
        context: &genpdf::Context,
        area: genpdf::render::Area<'_>,
        stil: genpdf::style::Style,
    ) -> Result<genpdf::RenderResult, genpdf::error::Error> {
        let zellen_groesse = area.size();
        let hintergrund_stil = genpdf::style::Style::new().with_color(self.farbe);
        let breite: f64 = zellen_groesse.width.into();
        let volle_hoehe: f64 = zellen_groesse.height.into();
        // Hintergrund nur bis zur maximalen Höhe zeichnen
        let hoehe: f64 = volle_hoehe.min(self.max_hoehe);
        let x_start = -self.erweiterung_links;
        let mut y = 0.0;
        // Hintergrund durch sehr dichte horizontale Linien (0,15 mm Abstand) simulieren
        while y <= hoehe - 0.5 {
            area.draw_line(
                vec![
                    genpdf::Position::new(x_start, y),
                    genpdf::Position::new(breite, y),
                ],
                hintergrund_stil,
            );
            y += 0.15;
        }
        // Inhalt über dem Hintergrund rendern
        self.inhalt.render(context, area, stil)
    }
}

// -- UI-Helfer --

/// Rendert eine einzelne Personenzeile (Name + Kürzel in eckigen Klammern + optionaler Lösch-Button).
/// Gibt `(wurde_gelöscht, Enter_gedrückt)` zurück, damit der Aufrufer reagieren kann.
fn personen_zeile(
    ui: &mut egui::Ui,
    person: &mut Person,
    show_delete: bool,
    request_focus: bool,
    text_color: Option<egui::Color32>,
) -> (bool, bool) {
    let mut deleted = false;
    let mut enter_pressed = false;
    ui.horizontal(|ui| {
        let available = ui.available_width();
        let kuerzel_w = 45.0;
        let bracket_space = 50.0; // [ ] und Spacing
        let delete_space = 28.0; // immer Platz reservieren
        let name_w = (available - kuerzel_w - bracket_space - delete_space).max(100.0);

        let mut name_edit = egui::TextEdit::singleline(&mut person.name)
            .hint_text(RichText::new("Name").font(egui::FontId::proportional(14.0)))
            .desired_width(name_w)
            .font(fette_schrift(14.0));
        if let Some(c) = text_color {
            name_edit = name_edit.text_color(c);
        }
        let name_r = ui.add(name_edit);
        if request_focus {
            name_r.request_focus();
        }
        if name_r.changed() {
            if person.name == "mz" {
                person.name = "Marcel Zimmer".to_string();
                // Cursor ans Ende setzen
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), name_r.id) {
                    let end = egui::text::CCursor::new(person.name.len());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ui.ctx(), name_r.id);
                }
            }
            if !person.kuerzel_manuell {
                person.kuerzel = Person::auto_kuerzel(&person.name);
            }
        }

        ui.label("[");
        let mut k_edit = egui::TextEdit::singleline(&mut person.kuerzel)
            .desired_width(kuerzel_w)
            .hint_text(RichText::new("Kürzel").font(egui::FontId::proportional(14.0)))
            .horizontal_align(egui::Align::Center)
            .font(fette_schrift(14.0));
        if let Some(c) = text_color {
            k_edit = k_edit.text_color(c);
        }
        let k_r = ui.add(k_edit);
        if k_r.changed() {
            person.kuerzel_manuell = !person.kuerzel.is_empty();
        }
        ui.label("]");

        if show_delete {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("×").color(egui::Color32::from_rgb(231, 76, 60)),
                    )
                    .small(),
                )
                .clicked()
            {
                deleted = true;
            }
        } else {
            ui.allocate_space(egui::vec2(20.0, 0.0));
        }

        enter_pressed = (name_r.lost_focus() || k_r.lost_focus())
            && ui.input(|i| i.key_pressed(egui::Key::Enter));
    });
    (deleted, enter_pressed)
}

/// Rendert eine linksbündige, fette Abschnittsüberschrift mit fixer Mindestbreite.
/// Optionale `farbe` überschreibt die Theme-Standardfarbe (für Omarchy-Theme).
fn abschnitts_beschriftung(ui: &mut egui::Ui, text: &str, label_w: f32, color: Option<egui::Color32>) {
    ui.allocate_ui_with_layout(
        egui::vec2(label_w, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(label_w);
            let mut rt = RichText::new(text).font(fette_schrift(14.0));
            if let Some(c) = color { rt = rt.color(c); }
            ui.label(rt);
        },
    );
}

/// Wie `abschnitts_beschriftung`, zeigt aber zusätzlich einen kleinen „+"-Button an.
/// Gibt `true` zurück, wenn der Button geklickt wurde (zum Hinzufügen einer weiteren Zeile).
fn abschnitts_beschriftung_mit_plus(ui: &mut egui::Ui, text: &str, label_w: f32, color: Option<egui::Color32>) -> bool {
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        egui::vec2(label_w, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(label_w);
            let mut rt = RichText::new(text).font(fette_schrift(14.0));
            if let Some(c) = color { rt = rt.color(c); }
            ui.label(rt);
            if ui.small_button("+").clicked() {
                clicked = true;
            }
        },
    );
    clicked
}

// -- UI --

impl eframe::App for ProtokollApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Event-Loop periodisch wecken für Wayland-Pings
        // (vsync: false in NativeOptions verhindert das Blockieren von eglSwapBuffers)
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        // Tastenkombinationen
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::N)) {
            let theme = self.theme;
            let has_omarchy = self.has_omarchy;
            let icon_texture = self.icon_texture.take();
            *self = ProtokollApp::new(ctx);
            self.theme = theme;
            self.has_omarchy = has_omarchy;
            self.icon_texture = icon_texture;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::O)) {
            self.laden();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.speichern();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::P)) {
            self.pdf_exportieren();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::W)) {
            self.show_quit_dialog = true;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::T)) {
            self.theme = self.theme.next(self.has_omarchy);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::H)) {
            url_oeffnen("https://www.marcelzimmer.de");
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::I)) {
            self.show_about_dialog = true;
        }

        // Ergebnisse von Datei-Dialogen verarbeiten
        if let Some(ref rx) = self.dialog_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    DialogErgebnis::Laden(path, content) => {
                        self.markdown_parsen(&content);
                        self.sort_personen();
                        self.save_path = Some(path);
                    }
                    DialogErgebnis::Speichern(path) => {
                        self.save_path = Some(path);
                    }
                    DialogErgebnis::PdfExport(path) => {
                        if let Some(font) = self.pending_pdf_font.take() {
                            self.pdf_generieren(&path, font);
                        }
                    }
                }
                self.dialog_rx = None;
            }
        }

        ctx.input_mut(|i| i.smooth_scroll_delta.y *= 10.0);

        self.input_text_color = None;
        self.label_color = None;
        match self.theme {
            Theme::Hell => ctx.set_visuals(egui::Visuals::light()),
            Theme::Dunkel => {
                let mut visuals = egui::Visuals::dark();
                visuals.panel_fill = egui::Color32::BLACK;
                visuals.window_fill = egui::Color32::BLACK;
                visuals.extreme_bg_color = egui::Color32::BLACK;
                let white_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
                visuals.widgets.noninteractive.fg_stroke = white_stroke;
                visuals.widgets.inactive.fg_stroke = white_stroke;
                visuals.widgets.hovered.fg_stroke = white_stroke;
                visuals.widgets.active.fg_stroke = white_stroke;
                ctx.set_visuals(visuals);
            }
            Theme::Omarchy => {
                let mut visuals = egui::Visuals::dark();
                if let Some(colors) = omarchy_farben_laden() {
                    // Hintergrund voll deckend (wie Terminal)
                    if let Some(bg) = colors.get("background") {
                        visuals.panel_fill = *bg;
                        visuals.window_fill = *bg;
                        visuals.extreme_bg_color = *bg;
                    }
                    // Hints → cursor (über noninteractive, wird automatisch abgedunkelt)
                    if let Some(cursor) = colors.get("cursor") {
                        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, *cursor);
                    }
                    // Buttons → accent
                    if let Some(accent) = colors.get("accent") {
                        let stroke = egui::Stroke::new(1.0, *accent);
                        visuals.widgets.inactive.fg_stroke = stroke;
                        visuals.widgets.hovered.fg_stroke = stroke;
                        visuals.widgets.active.fg_stroke = stroke;
                        visuals.selection.bg_fill = *accent;
                        visuals.hyperlink_color = *accent;
                        visuals.widgets.hovered.bg_fill = accent.linear_multiply(0.3);
                    }
                    // Beschriftungen/Labels → color3
                    if let Some(label) = colors.get("color3") {
                        self.label_color = Some(*label);
                    }
                    // Eingegebener Text in Textfeldern → color2
                    if let Some(text_color) = colors.get("color2") {
                        self.input_text_color = Some(*text_color);
                    }
                } else {
                    visuals.panel_fill = egui::Color32::from_rgb(30, 30, 30);
                    visuals.window_fill = egui::Color32::from_rgb(30, 30, 30);
                }
                ctx.set_visuals(visuals);
            }
        }

        let alle_kuerzel = self.alle_kuerzel();
        // Feste Breite der linksseitigen Abschnittsbezeichnungen (in Pixeln)
        let beschriftungs_breite = 160.0;

        let panel_frame = egui::Frame::central_panel(&ctx.style())
            .inner_margin(egui::Margin::same(10));
        egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
            // Toolbar oben rechts: Beenden-Button + Hamburger-Menü
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                if ui.button(RichText::new("X").size(14.0)).on_hover_text("Beenden (Strg+W)").clicked() {
                    self.show_quit_dialog = true;
                }

                let menu_items: &[(&str, &str, i32)] = &[
                    ("Neu", "Strg+N", 0),
                    ("Öffnen", "Strg+O", 0),
                    ("Speichern", "Strg+S", 0),
                    ("PDF erzeugen", "Strg+P", 0),
                    ("", "", 1), // separator
                    ("Theme ändern", "Strg+T", 0),
                    ("", "", 1), // separator
                    ("Hilfe", "Strg+H", 0),
                    ("Über", "Strg+I", 0),
                ];
                egui::menu::menu_button(ui, RichText::new("☰").size(14.0), |ui| {
                    ui.set_width(180.0);
                    for &(label, shortcut, is_sep) in menu_items {
                        if is_sep == 1 {
                            ui.separator();
                            continue;
                        }
                        let w = ui.available_width();
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(w, 24.0),
                            egui::Sense::click(),
                        );
                        if ui.is_rect_visible(rect) {
                            // Hover-Highlight
                            if response.hovered() {
                                ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
                            }
                            // Label links
                            ui.painter().text(
                                rect.left_center() + egui::vec2(8.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                label,
                                egui::FontId::proportional(13.0),
                                ui.visuals().text_color(),
                            );
                            // Shortcut rechts
                            if !shortcut.is_empty() {
                                ui.painter().text(
                                    rect.right_center() - egui::vec2(8.0, 0.0),
                                    egui::Align2::RIGHT_CENTER,
                                    shortcut,
                                    egui::FontId::proportional(12.0),
                                    ui.visuals().weak_text_color(),
                                );
                            }
                        }
                        let clicked = response.clicked();
                        if clicked {
                            match label {
                                "Neu" => {
                                    let theme = self.theme;
                                    let has_omarchy = self.has_omarchy;
                                    let icon_texture = self.icon_texture.take();
                                    *self = ProtokollApp::new(ctx);
                                    self.theme = theme;
                                    self.has_omarchy = has_omarchy;
                                    self.icon_texture = icon_texture;
                                }
                                "Öffnen" => self.laden(),
                                "Speichern" => self.speichern(),
                                "PDF erzeugen" => self.pdf_exportieren(),
                                "Theme ändern" => self.theme = self.theme.next(self.has_omarchy),
                                "Hilfe" => {
                                    url_oeffnen("https://www.marcelzimmer.de");
                                }
                                "Über" => self.show_about_dialog = true,
                                _ => {}
                            }
                            ui.close_menu();
                        }
                    }
                });
            });

            // Kurzreferenz auf die aktuellen Theme-Farben (für Textfelder und Labels)
            let textfarbe = self.input_text_color;

            // Header-Bereich (fixiert, scrollt nicht mit)
            {
                // 11: Projekt
                let mut projekt_edit = egui::TextEdit::singleline(&mut self.projekt)
                    .hint_text(RichText::new("Projektname").font(egui::FontId::proportional(13.0)))
                    .desired_width(400.0)
                    .font(fette_schrift(13.0));
                if let Some(c) = textfarbe { projekt_edit = projekt_edit.text_color(c); }
                ui.add(projekt_edit);

                ui.add_space(4.0);

                // Titel
                let mut titel_edit = egui::TextEdit::singleline(&mut self.titel)
                    .font(fette_schrift(28.0))
                    .hint_text(RichText::new("Titel").font(egui::FontId::proportional(28.0)))
                    .desired_width(ui.available_width());
                if let Some(c) = textfarbe { titel_edit = titel_edit.text_color(c); }
                ui.add(titel_edit);

                ui.add_space(6.0);

                // Datum + Ort
                ui.horizontal(|ui| {
                    let mut datum_edit = egui::TextEdit::singleline(&mut self.datum_text)
                        .desired_width(250.0)
                        .hint_text(RichText::new("Wochentag, TT.MM.JJJJ").font(egui::FontId::proportional(14.0)))
                        .font(fette_schrift(14.0));
                    if let Some(c) = textfarbe { datum_edit = datum_edit.text_color(c); }
                    ui.add(datum_edit);
                    ui.label(RichText::new("|").size(15.0));
                    let mut ort_edit = egui::TextEdit::singleline(&mut self.ort)
                        .desired_width(ui.available_width())
                        .hint_text(RichText::new("Ort").font(egui::FontId::proportional(14.0)))
                        .font(fette_schrift(14.0));
                    if let Some(c) = textfarbe { ort_edit = ort_edit.text_color(c); }
                    ui.add(ort_edit);
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let beschriftungsfarbe = self.label_color;

                // 12: Protokollführer (nebeneinander)
                ui.horizontal_top(|ui| {
                    abschnitts_beschriftung(ui, "Protokollführer", beschriftungs_breite,self.label_color);
                    personen_zeile(ui, &mut self.protokollant, false, false, self.input_text_color);
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // 13: Teilnehmer (nebeneinander, Enter → neue Zeile)
                let mut tn_add = false;
                let mut tn_remove: Option<usize> = None;
                ui.horizontal_top(|ui| {
                    if abschnitts_beschriftung_mit_plus(ui, "Teilnehmer", beschriftungs_breite,self.label_color) {
                        self.teilnehmer.push(Person::new());
                    }
                    let tn_len = self.teilnehmer.len();
                    ui.vertical(|ui| {
                        for i in 0..tn_len {
                            let is_last = i == tn_len - 1;
                            let focus = is_last && self.focus_new_teilnehmer;
                            let (del, enter) =
                                personen_zeile(ui, &mut self.teilnehmer[i], tn_len > 1, focus, self.input_text_color);
                            if focus {
                                self.focus_new_teilnehmer = false;
                            }
                            if del {
                                tn_remove = Some(i);
                            }
                            if enter {
                                tn_add = true;
                            }
                        }
                    });
                });
                if let Some(idx) = tn_remove {
                    self.teilnehmer.remove(idx);
                }
                if tn_add {
                    self.teilnehmer.push(Person::new());
                    self.focus_new_teilnehmer = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // 13: Zur Kenntnis (nebeneinander)
                let mut zk_add = false;
                let mut zk_remove: Option<usize> = None;
                ui.horizontal_top(|ui| {
                    if abschnitts_beschriftung_mit_plus(ui, "Zur Kenntnis", beschriftungs_breite,self.label_color) {
                        self.zur_kenntnis.push(Person::new());
                    }
                    let zk_len = self.zur_kenntnis.len();
                    ui.vertical(|ui| {
                        for i in 0..zk_len {
                            let is_last = i == zk_len - 1;
                            let focus = is_last && self.focus_new_zur_kenntnis;
                            let (del, enter) =
                                personen_zeile(ui, &mut self.zur_kenntnis[i], zk_len > 1, focus, self.input_text_color);
                            if focus {
                                self.focus_new_zur_kenntnis = false;
                            }
                            if del {
                                zk_remove = Some(i);
                            }
                            if enter {
                                zk_add = true;
                            }
                        }
                    });
                });
                if let Some(idx) = zk_remove {
                    self.zur_kenntnis.remove(idx);
                }
                if zk_add {
                    self.zur_kenntnis.push(Person::new());
                    self.focus_new_zur_kenntnis = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // 14: Über dieses Meeting
                ui.horizontal_top(|ui| {
                    abschnitts_beschriftung(ui, "Über dieses Meeting", beschriftungs_breite,self.label_color);
                    let mut meeting_edit = egui::TextEdit::multiline(&mut self.ueber_meeting)
                        .hint_text(RichText::new("Informationen zum Meeting").font(egui::FontId::proportional(14.0)))
                        .desired_width(ui.available_width())
                        .desired_rows(3)
                        .font(fette_schrift(14.0));
                    if let Some(c) = textfarbe { meeting_edit = meeting_edit.text_color(c); }
                    ui.add(meeting_edit);
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    abschnitts_beschriftung(ui, "Status", beschriftungs_breite,self.label_color);
                    let prev_entwurf = self.ist_entwurf;
                    let prev_freigegeben = self.ist_freigegeben;
                    let entwurf_label = {
                        let mut rt = RichText::new("Entwurf").font(fette_schrift(14.0));
                        if let Some(c) = textfarbe { rt = rt.color(c); }
                        rt
                    };
                    let freigegeben_label = {
                        let mut rt = RichText::new("Freigegeben").font(fette_schrift(14.0));
                        if let Some(c) = textfarbe { rt = rt.color(c); }
                        rt
                    };
                    let cb_w = 140.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(cb_w, ui.spacing().interact_size.y),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(cb_w);
                            ui.checkbox(&mut self.ist_entwurf, entwurf_label);
                        },
                    );
                    ui.checkbox(&mut self.ist_freigegeben, freigegeben_label);
                    if self.ist_entwurf && !prev_entwurf {
                        self.ist_freigegeben = false;
                    }
                    if self.ist_freigegeben && !prev_freigegeben {
                        self.ist_entwurf = false;
                    }
                    if !self.ist_entwurf && prev_entwurf {
                        self.ist_freigegeben = true;
                    }
                    if !self.ist_freigegeben && prev_freigegeben {
                        self.ist_entwurf = true;
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    abschnitts_beschriftung(ui, "Klassifizierung", beschriftungs_breite,self.label_color);
                    let cb_w = 140.0;
                    let sicherheiten = Sicherheit::all();
                    let last_idx = sicherheiten.len() - 1;
                    for (idx, s) in sicherheiten.iter().enumerate() {
                        let mut checked = self.sicherheit == *s;
                        let label = {
                            let mut rt = RichText::new(s.label()).font(fette_schrift(14.0));
                            if let Some(c) = textfarbe { rt = rt.color(c); }
                            rt
                        };
                        if idx < last_idx {
                            let clicked = ui.allocate_ui_with_layout(
                                egui::vec2(cb_w, ui.spacing().interact_size.y),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(cb_w);
                                    ui.checkbox(&mut checked, label).clicked()
                                },
                            ).inner;
                            if clicked {
                                if checked { self.sicherheit = s.clone(); }
                                else { self.sicherheit = Sicherheit::Intern; }
                            }
                        } else {
                            if ui.checkbox(&mut checked, label).clicked() {
                                if checked { self.sicherheit = s.clone(); }
                                else { self.sicherheit = Sicherheit::Intern; }
                            }
                        }
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Einträge-Tabelle
                let mut entry_remove: Option<usize> = None;
                let mut entry_swap: Option<(usize, usize)> = None;
                let entry_len = self.eintraege.len();

                let available = ui.available_width();
                let punkt_w: f32 = 160.0;
                let art_w: f32 = 140.0;
                let kum_text_w: f32 = 130.0;
                let kum_dd_w: f32 = 35.0;
                let bis_w: f32 = 88.0;
                let action_w: f32 = 76.0;
                let col_sp: f32 = 8.0;
                let gaps = 5.0 * col_sp;
                let notiz_w = (available
                    - punkt_w
                    - art_w
                    - (kum_text_w + kum_dd_w + 4.0)
                    - bis_w
                    - action_w
                    - gaps
                    - 16.0)
                    .max(150.0);

                let mut header_line_y: f32 = 0.0;

                ui.add_space(12.0);

                let line_x_range = ui.cursor().left()..=ui.available_rect_before_wrap().right();

                let prev_notiz_focus = self.notiz_had_focus.take();
                let mut new_notiz_focus: Option<(usize, usize)> = None;

                let _grid_resp = egui::Grid::new("eintraege")
                    .num_columns(6)
                    .spacing([col_sp, 6.0])
                    .striped(false)
                    .show(ui, |ui| {
                        // Kopfzeile — linksbündig, erzwingt Spaltenbreiten
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.set_min_width(punkt_w);
                            ui.label(RichText::new("").size(14.0));
                        });
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.set_min_width(art_w);
                            let mut rt = RichText::new("Art").font(fette_schrift(14.0));
                            if let Some(c) = beschriftungsfarbe { rt = rt.color(c); }
                            ui.label(rt);
                        });
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.set_min_width(notiz_w);
                            let mut rt = RichText::new("Notiz").font(fette_schrift(14.0));
                            if let Some(c) = beschriftungsfarbe { rt = rt.color(c); }
                            ui.label(rt);
                        });
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.set_min_width(kum_text_w + kum_dd_w + 4.0);
                            let mut rt = RichText::new("Kümmerer").font(fette_schrift(14.0));
                            if let Some(c) = beschriftungsfarbe { rt = rt.color(c); }
                            ui.label(rt);
                        });
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.set_min_width(bis_w);
                            let mut rt = RichText::new("Bis").font(fette_schrift(14.0));
                            if let Some(c) = beschriftungsfarbe { rt = rt.color(c); }
                            ui.label(rt);
                        });
                        ui.label("");
                        ui.end_row();

                        header_line_y = ui.cursor().top();

                        // Spacer-Zeile für Abstand zwischen Linie und Daten
                        ui.add_sized([0.0, 6.0], egui::Label::new(""));
                        ui.add_sized([0.0, 6.0], egui::Label::new(""));
                        ui.add_sized([0.0, 6.0], egui::Label::new(""));
                        ui.add_sized([0.0, 6.0], egui::Label::new(""));
                        ui.add_sized([0.0, 6.0], egui::Label::new(""));
                        ui.label("");
                        ui.end_row();

                        for i in 0..entry_len {
                            let is_todo = self.eintraege[i].art == Art::Todo;

                            // 4: Punkt (oben ausgerichtet)
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                let mut punkt_edit = egui::TextEdit::singleline(&mut self.eintraege[i].punkt)
                                    .hint_text(RichText::new(if is_todo { "" } else { "Punkt" }).font(egui::FontId::proportional(14.0)))
                                    .font(fette_schrift(14.0))
                                    .interactive(!is_todo)
                                    .frame(!is_todo);
                                if let Some(c) = textfarbe { punkt_edit = punkt_edit.text_color(c); }
                                ui.add_sized([punkt_w, 20.0], punkt_edit);
                            });

                            // 8: Art-Dropdown (oben ausgerichtet)
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                let sel = RichText::new(self.eintraege[i].art.selected_label())
                                    .color(self.eintraege[i].art.color())
                                    .font(fette_schrift(14.0));
                                egui::ComboBox::from_id_salt(format!("art_{i}"))
                                    .selected_text(sel)
                                    .width(art_w)
                                    .show_ui(ui, |ui| {
                                        let prev_art = self.eintraege[i].art.clone();
                                        for art in Art::all() {
                                            let txt = RichText::new(art.label()).color(art.color()).font(fette_schrift(14.0));
                                            ui.selectable_value(
                                                &mut self.eintraege[i].art,
                                                art.clone(),
                                                txt,
                                            );
                                        }
                                        if self.eintraege[i].art == Art::Todo && prev_art != Art::Todo {
                                            self.eintraege[i].punkt.clear();
                                        }
                                    });
                            });

                            // 3: Notiz — dynamische Höhe + Cursor-Navigation
                            let notiz_id = egui::Id::new(("notiz", i));
                            let notiz_rows = self.eintraege[i].notiz.lines().count().max(1);
                            let mut notiz_edit = egui::TextEdit::multiline(&mut self.eintraege[i].notiz)
                                .id(notiz_id)
                                .hint_text(RichText::new("Notiz").font(egui::FontId::proportional(14.0)))
                                .desired_width(notiz_w)
                                .desired_rows(notiz_rows)
                                .font(fette_schrift(14.0));
                            if let Some(c) = textfarbe { notiz_edit = notiz_edit.text_color(c); }
                            let notiz_resp = ui.add(notiz_edit);
                            if self.focus_notiz == Some(i) {
                                notiz_resp.request_focus();
                                self.focus_notiz = None;
                            }
                            if notiz_resp.has_focus() {
                                if let Some(state) = egui::TextEdit::load_state(ui.ctx(), notiz_id) {
                                    if let Some(range) = state.cursor.char_range() {
                                        new_notiz_focus = Some((i, range.primary.index));
                                    }
                                }
                            }

                            // 5+7+10: Kümmerer (oben ausgerichtet, nur bei TODO sichtbar)
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                ui.horizontal(|ui| {
                                    let mut kum_edit = egui::TextEdit::singleline(
                                            &mut self.eintraege[i].kuemmerer,
                                        )
                                        .hint_text(RichText::new(if is_todo { "Wer?" } else { "" }).font(egui::FontId::proportional(14.0)))
                                        .desired_width(kum_text_w)
                                        .interactive(is_todo)
                                        .frame(is_todo)
                                        .font(fette_schrift(14.0));
                                    if let Some(c) = textfarbe { kum_edit = kum_edit.text_color(c); }
                                    ui.add(kum_edit);
                                    if is_todo {
                                        egui::ComboBox::from_id_salt(format!("kum_sel_{i}"))
                                            .selected_text("")
                                            .width(kum_dd_w)
                                            .show_ui(ui, |ui| {
                                                if alle_kuerzel.is_empty() {
                                                    ui.label("Keine Kürzel");
                                                }
                                                for k in &alle_kuerzel {
                                                    if ui
                                                        .selectable_label(
                                                            self.eintraege[i].kuemmerer == *k,
                                                            k,
                                                        )
                                                        .clicked()
                                                    {
                                                        self.eintraege[i].kuemmerer = k.clone();
                                                    }
                                                }
                                            });
                                    } else {
                                        ui.add_space(kum_dd_w + 4.0);
                                    }
                                });
                            });

                            // 6: Bis (oben ausgerichtet, nur bei TODO sichtbar, mit Datumsvalidierung)
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                let bis_valid = self.eintraege[i].bis.is_empty()
                                    || NaiveDate::parse_from_str(
                                        &self.eintraege[i].bis,
                                        "%d.%m.%Y",
                                    )
                                    .is_ok();
                                let bis_color = if !bis_valid {
                                    egui::Color32::from_rgb(231, 76, 60)
                                } else if let Some(c) = textfarbe {
                                    c
                                } else {
                                    ui.visuals().text_color()
                                };
                                ui.add_sized(
                                    [bis_w, 20.0],
                                    egui::TextEdit::singleline(&mut self.eintraege[i].bis)
                                        .hint_text(RichText::new(if is_todo { "TT.MM.JJJJ" } else { "" }).font(egui::FontId::proportional(14.0)))
                                        .text_color(bis_color)
                                        .interactive(is_todo)
                                        .frame(is_todo)
                                        .font(fette_schrift(14.0)),
                                );
                            });

                            // Aktionen: Hoch / Runter / Löschen
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    if i > 0 {
                                        if ui.add_sized([20.0, 20.0], egui::Button::new("▲")).clicked() {
                                            entry_swap = Some((i, i - 1));
                                        }
                                    } else {
                                        ui.add_sized([20.0, 20.0], egui::Label::new(""));
                                    }
                                    ui.add_space(2.0);
                                    if i + 1 < entry_len {
                                        if ui.add_sized([20.0, 20.0], egui::Button::new("▼")).clicked() {
                                            entry_swap = Some((i, i + 1));
                                        }
                                    } else {
                                        ui.add_sized([20.0, 20.0], egui::Label::new(""));
                                    }
                                    ui.add_space(10.0);
                                    if entry_len > 1 {
                                        if ui.add_sized([20.0, 20.0], egui::Button::new(
                                            RichText::new("×").color(egui::Color32::from_rgb(231, 76, 60)),
                                        )).clicked() {
                                            entry_remove = Some(i);
                                        }
                                    }
                                });
                            });
                            ui.end_row();
                        }
                    });

                // Cursor hoch/runter zwischen Notiz-Feldern
                {
                    let up = ui.input(|inp| inp.key_pressed(egui::Key::ArrowUp));
                    let down = ui.input(|inp| inp.key_pressed(egui::Key::ArrowDown));
                    if let Some((prev_i, prev_cursor)) = prev_notiz_focus {
                        if prev_i < self.eintraege.len() {
                            let text = &self.eintraege[prev_i].notiz;
                            let mut safe_idx = prev_cursor.min(text.len());
                            while safe_idx > 0 && !text.is_char_boundary(safe_idx) {
                                safe_idx -= 1;
                            }
                            let on_first = !text[..safe_idx].contains('\n');
                            let on_last = !text[safe_idx..].contains('\n');
                            if up && on_first && prev_i > 0 {
                                self.focus_notiz = Some(prev_i - 1);
                            } else if down && on_last && prev_i + 1 < self.eintraege.len() {
                                self.focus_notiz = Some(prev_i + 1);
                            }
                        }
                    }
                    self.notiz_had_focus = new_notiz_focus;
                }

                // 15: Linie unter Kopfzeile (gleiche Breite wie Separators)
                ui.painter().hline(
                    line_x_range,
                    header_line_y - 1.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(180, 180, 180)),
                );

                if let Some((a, b)) = entry_swap {
                    self.eintraege.swap(a, b);
                }
                if let Some(idx) = entry_remove {
                    self.eintraege.remove(idx);
                }

                ui.add_space(8.0);
                if ui.button(RichText::new("+ Eintrag hinzufügen").strong()).clicked() {
                    self.eintraege.push(Eintrag::new());
                }
            });
        });

        // Über-Dialog
        if self.show_about_dialog {
            let mut open = true;
            egui::Window::new("Über MZProtokoll")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(350.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);

                        // Logo
                        let texture = self.icon_texture.get_or_insert_with(|| {
                            let png_bytes = include_bytes!("../assets/icon.png");
                            let image = image::load_from_memory(png_bytes).expect("Failed to load icon");
                            let rgba = image.to_rgba8();
                            let size = [rgba.width() as usize, rgba.height() as usize];
                            let pixels = rgba.into_raw();
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                            ctx.load_texture("app-icon", color_image, egui::TextureOptions::LINEAR)
                        });
                        ui.image((texture.id(), egui::vec2(80.0, 80.0)));

                        ui.add_space(12.0);

                        // App-Name
                        ui.label(RichText::new("MZProtokoll").strong().size(24.0).color(egui::Color32::WHITE));

                        ui.add_space(4.0);
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));

                        ui.add_space(20.0);

                        if ui.add(egui::Button::new("Report an Issue").min_size(egui::vec2(200.0, 32.0))).clicked() {
                            url_oeffnen("https://www.marcelzimmer.de");
                        }
                        ui.add_space(4.0);
                        if ui.add(egui::Button::new("Follow on X").min_size(egui::vec2(200.0, 32.0))).clicked() {
                            url_oeffnen("https://www.x.com/marcelzimmer");
                        }

                        ui.add_space(8.0);
                    });
                });
            if !open {
                self.show_about_dialog = false;
            }
        }

        // PDF-Fehler-Dialog
        if self.show_pdf_error {
            egui::Window::new("PDF-Export nicht möglich")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    ui.label("Keine passende Schriftart gefunden.");
                    ui.add_space(4.0);
                    ui.label("Bitte installiere eine der folgenden Schriftarten:");
                    ui.label("  - Liberation Sans");
                    ui.label("  - Noto Sans");
                    ui.add_space(12.0);
                    ui.vertical_centered(|ui| {
                        if ui.add(egui::Button::new(RichText::new("OK").strong()).min_size(egui::vec2(120.0, 30.0))).clicked() {
                            self.show_pdf_error = false;
                        }
                    });
                });
        }

        // Pflichtfeld-Hinweis
        if self.show_pflichtfeld_hinweis {
            egui::Window::new("Pflichtfeld")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    ui.label("Bitte den Protokollführer eintragen.");
                    ui.add_space(12.0);
                    ui.vertical_centered(|ui| {
                        if ui.add(egui::Button::new(RichText::new("OK").strong()).min_size(egui::vec2(120.0, 30.0))).clicked() {
                            self.show_pflichtfeld_hinweis = false;
                        }
                    });
                });
        }

        // Beenden-Dialog
        if self.show_quit_dialog {
            egui::Window::new("Beenden")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Möchten Sie die Anwendung wirklich beenden?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Ja").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Nein").clicked() {
                            self.show_quit_dialog = false;
                        }
                    });
                });
        }
    }
}
