# MZProtokoll – Entwicklerdokumentation

**Autor:** Marcel Zimmer<br>
**Web:** [www.marcelzimmer.de](https://www.marcelzimmer.de)<br>
**X:** [@marcelzimmer](https://x.com/marcelzimmer)<br>
**GitHub:** [@marcelzimmer](https://github.com/marcelzimmer)<br>
**Version:** 1.0.0<br>
**Sprache:** Rust<br>
**Lizenz:** MIT

---

## Inhaltsverzeichnis

1. [Überblick](#überblick)
2. [Abhängigkeiten](#abhängigkeiten)
3. [Projektstruktur](#projektstruktur)
4. [Datenmodell](#datenmodell)
5. [Architektur und Programmfluss](#architektur-und-programmfluss)
6. [UI-Schicht](#ui-schicht)
7. [Schriftarten-Laden](#schriftarten-laden)
8. [Markdown-Export und -Import](#markdown-export-und--import)
9. [PDF-Export](#pdf-export)
10. [Theme-System](#theme-system)
11. [Datei-Dialoge und Thread-Kommunikation](#datei-dialoge-und-thread-kommunikation)
12. [Tastenkombinationen](#tastenkombinationen)
13. [Erweiterungsmöglichkeiten](#erweiterungsmöglichkeiten)
14. [Build und Installation](#build-und-installation)

---

## Überblick

MZProtokoll ist eine Linux App zum Erstellen und Exportieren von Meeting-Protokollen [Markdown & PDF]. Die Oberfläche wird mit **egui/eframe** (Rust-GUI-Framework)
gerendert; als Ausgabeformat stehen **Markdown** (maschinenlesbar, versionierbar) und **PDF**
(druckfertig, mit Seitenzahlen und Linkverzeichnis) zur Verfügung.

Die gesamte Anwendungslogik befindet sich in einer einzigen Quelldatei: `src/main.rs`.

---

## Abhängigkeiten

| Crate    | Version | Verwendungszweck                                          |
|----------|---------|-----------------------------------------------------------|
| `eframe` | 0.31    | Anwendungsrahmen und Ereignisschleife (egui-Backend)      |
| `egui`   | —       | Immediate-Mode-GUI (Teil von eframe)                      |
| `chrono` | 0.4     | Aktuelles Datum, Wochentag, Zeitstempel                   |
| `rfd`    | 0.15    | Datei-Öffnen/Speichern-Dialoge (plattformnativ)           |
| `genpdf` | 0.2     | PDF-Dokument-Generierung                                  |
| `image`  | 0.25.9  | PNG-Icon für den Über-Dialog einlesen                     |

---

## Projektstruktur

```
mzprotokoll/
├── src/
│   └── main.rs          – gesamte Anwendungslogik (Datenmodell, UI, Export)
├── assets/
│   └── icon.png         – App-Icon (wird zur Compile-Zeit eingebettet)
├── Cargo.toml           – Paketdefinition und Abhängigkeiten
├── Cargo.lock           – reproduzierbare Builds
├── install.sh           – Installations-Skript (Linux)
├── mzprotokoll.desktop  – Desktop-Entry-Datei (Linux-Starter)
├── LICENSE              – MIT-Lizenz
└── README.md            – diese Datei
```

---

## Datenmodell

### `ProtokollApp` (Hauptzustand)

Zentrale Struct, die den vollständigen Anwendungszustand hält. Sie implementiert
`eframe::App` und wird von der egui-Ereignisschleife verwaltet.

**Protokoll-Kopfdaten:**

| Feld            | Typ                | Bedeutung                                        |
|-----------------|--------------------|--------------------------------------------------|
| `projekt`       | `String`           | Optionaler Projektname über dem Titel            |
| `titel`         | `String`           | Meeting-Titel (Hauptüberschrift)                 |
| `datum_text`    | `String`           | Datum als Freitext (z. B. „Montag, 05.02.2026") |
| `ort`           | `String`           | Veranstaltungsort                                |
| `protokollant`  | `Person`           | Protokollführer (Pflichtfeld)                    |
| `teilnehmer`    | `Vec<Person>`      | Liste der Meetingteilnehmer                      |
| `zur_kenntnis`  | `Vec<Person>`      | Personen, die das Protokoll erhalten             |
| `ueber_meeting` | `String`           | Freitext-Beschreibung des Meetings               |
| `ist_entwurf`   | `bool`             | Status: Entwurf                                  |
| `ist_freigegeben` | `bool`           | Status: Freigegeben                              |
| `sicherheit`    | `Sicherheit`       | Klassifizierungsstufe                            |
| `eintraege`     | `Vec<Eintrag>`     | Alle Tabelleneinträge                            |

### `Person`

```rust
struct Person {
    name: String,           // vollständiger Name
    kuerzel: String,        // Kürzel für TODO-Einträge (z. B. „MZ")
    kuerzel_manuell: bool,  // verhindert automatische Kürzel-Ableitung
}
```

Das Kürzel wird automatisch aus den Anfangsbuchstaben des Namens gebildet
(`Person::auto_kuerzel`), sofern `kuerzel_manuell = false`.

### `Eintrag`

```rust
struct Eintrag {
    punkt: String,     // Tagesordnungspunkt (leer bei Art::Todo)
    art: Art,          // Typ des Eintrags
    notiz: String,     // Freitext, Markdown-Links erlaubt
    kuemmerer: String, // Kürzel der verantwortlichen Person (nur Todo)
    bis: String,       // Fälligkeitsdatum TT.MM.JJJJ (nur Todo)
}
```

### `Art` (Eintragstyp)

| Variante      | Farbe      | Felder aktiv      |
|---------------|------------|-------------------|
| `Leer`        | Grau       | —                 |
| `Abgebrochen` | Rot        | Punkt, Notiz      |
| `Agenda`      | Lila       | Punkt, Notiz      |
| `Entscheidung`| Blau       | Punkt, Notiz      |
| `Fertig`      | Grün       | Punkt, Notiz      |
| `Idee`        | Gelb       | Punkt, Notiz      |
| `Info`        | Grau       | Punkt, Notiz      |
| `Todo`        | Orange     | Notiz, Kümmerer, Bis |

Bei `Art::Todo` wird der Punkt-Text automatisch geleert und die Felder
„Kümmerer" und „Bis" werden editierbar.

### `Sicherheit` (Klassifizierungsstufe)

`Oeffentlich` → `Intern` → `Vertraulich` → `StrengVertraulich`

### `DialogErgebnis`

Kommunikationstyp zwischen Datei-Dialog-Threads und dem Haupt-Thread:

```rust
enum DialogErgebnis {
    Laden(PathBuf, String),   // Pfad + Dateiinhalt
    Speichern(PathBuf),       // gewählter Speicherpfad
    PdfExport(PathBuf),       // gewählter PDF-Speicherpfad
}
```

---

## Architektur und Programmfluss

MZProtokoll folgt dem **Immediate-Mode-GUI-Muster** von egui:

```
┌────────────────────────────────────────┐
│  eframe-Ereignisschleife               │
│  (läuft ~60 Hz oder bei Ereignis)      │
│                                        │
│  ProtokollApp::update()                │
│  ┌──────────────────────────────────┐  │
│  │ 1. Tastenkombinationen prüfen    │  │
│  │ 2. Dialog-Ergebnisse verarbeiten │  │
│  │ 3. Theme anwenden                │  │
│  │ 4. UI rendern (deklarativ)       │  │
│  │ 5. Dialoge anzeigen              │  │
│  └──────────────────────────────────┘  │
└────────────────────────────────────────┘
         │ Nutzeraktion (Klick/Eingabe)
         ▼
   Zustandsänderung in ProtokollApp
         │
         ▼
   nächster Frame → neu rendern
```

### Datei-Dialoge (Thread-Kommunikation)

Da native Datei-Dialoge (`rfd`) den UI-Thread blockieren würden, werden sie in
separaten Threads ausgeführt. Ergebnisse werden über einen `mpsc`-Kanal
zurückgegeben und im nächsten `update()`-Aufruf ausgewertet:

```
Haupt-Thread               Dialog-Thread
     │                          │
     │── mpsc::channel() ──────►│
     │                          │
     │   (UI läuft weiter)      │── rfd::FileDialog::new()...
     │                          │        (blockiert hier)
     │◄── tx.send(Ergebnis) ────│
     │                          │
     │── dialog_rx.try_recv() ──┤
     │   Ergebnis verarbeiten   │
```

---

## UI-Schicht

### Aufbau der Oberfläche

```
┌─────────────────────────────────────────────────────┐
│ [Projektname]                        [☰] [X]        │
│ [Titel des Meetings]                                │
│ [Datum]  |  [Ort]                                   │
│ ─────────────────────────────────────────           │
│ ScrollArea:                                         │
│   Protokollführer  [Name]              [Kürzel]     │
│   ─────────────────────────────────────────         │
│   Teilnehmer [+]   [Name]              [Kürzel] [×] │
│   ─────────────────────────────────────────         │
│   Zur Kenntnis [+] [Name]              [Kürzel] [×] │
│   ─────────────────────────────────────────         │
│   Über dieses Meeting  [Freitext...]                │
│   ─────────────────────────────────────────         │
│   Status    [✓] Entwurf  [ ] Freigegeben            │
│   ─────────────────────────────────────────         │
│   Klassifizierung [ ] Öff  [✓] Int  [ ] Vertr ...   │
│   ─────────────────────────────────────────         │
│   Eintrags-Tabelle:                                 │
│   ┌──────────┬────────┬────────┬────────┬─────┬──┐  │
│   │ Punkt    │ Art    │ Notiz  │Kümmerer│ Bis │  │  │
│   ├──────────┼────────┼────────┼────────┼─────┼──┤  │
│   │ ...      │ TODO ▼ │ ...    │ MZ  ▼  │...  │▲▼×│ │
│   └──────────┴────────┴────────┴────────┴─────┴──┘  │
│   [+ Eintrag hinzufügen]                            │
└─────────────────────────────────────────────────────┘
```

### UI-Hilfsfunktionen

| Funktion                              | Beschreibung                                                   |
|---------------------------------------|----------------------------------------------------------------|
| `fette_schrift(groesse)`              | Erstellt eine `egui::FontId` für die „Bold"-Familie            |
| `personen_zeile(ui, person, ...)`     | Rendert eine Name+Kürzel-Zeile, gibt (gelöscht, Enter) zurück  |
| `abschnitts_beschriftung(ui, ...)`    | Linksbündige fette Überschrift mit fixer Mindestbreite         |
| `abschnitts_beschriftung_mit_plus(…)` | Wie oben, aber mit „+"-Button; gibt `true` bei Klick zurück    |

### Eintrags-Tabelle

Die Tabelle wird als `egui::Grid` mit 6 Spalten gerendert:
`Punkt | Art | Notiz | Kümmerer | Bis | Aktionen`

Besonderheit: Bei `Art::Todo` werden Punkt-Feld (inaktiv) und Kümmerer/Bis-Felder (aktiv)
gerendert. Bei anderen Typen ist es umgekehrt.

**Cursor-Navigation zwischen Notizfeldern:** Pfeiltasten `↑`/`↓` springen aus dem
obersten/untersten Zeilende eines Notizfeldes ins vorherige/nächste Notizfeld.
Die Implementierung speichert jedes Frame in `notiz_had_focus` den letzten Fokus-Index
und die Cursor-Position, um im nächsten Frame die Navigation auswerten zu können.

---

## Schriftarten-Laden

egui benötigt für die Anzeige von fettem Text eine separate Font-Family „Bold".
Die Anwendung versucht beim Start Systemschriften in dieser Reihenfolge zu laden:

- Liberation Sans (Linux: Arch, Fedora, Debian, Ubuntu)
- Noto Sans (Linux)
- DejaVu Sans (Linux-Fallback)

Wird keine Schrift gefunden, verwendet egui seine eingebettete Fallback-Schrift
(ohne fette Variante).

---

## Markdown-Export und -Import

### Dateiformat

Das MZProtokoll-Markdown-Format ist ein strukturiertes, abschnittsbasiertes Markdown:

```markdown
**Projekt:** Projektname

# Titel des Meetings

**Datum:** Montag, 05.02.2026 | **Ort:** Berlin

---

## Protokollführer

Marcel Zimmer [MZ]

## Teilnehmer

- Anna Beispiel [AB]
- Bob Muster [BM]

## Zur Kenntnis

- Carol Test [CT]

## Über dieses Meeting

Kurzbeschreibung des Meetings.

## Status

- [x] Entwurf
- [ ] Freigegeben

## Klassifizierung

- [ ] Öffentlich
- [x] Intern
- [ ] Vertraulich
- [ ] Streng vertraulich

---

## Einträge

| Punkt | Art | Notiz | Kümmerer | Bis |
|-------|-----|-------|----------|-----|
| Beispielpunkt | INFO | Notiz zum Punkt | | |
| | TODO | Aufgabe erledigen | MZ | 31.12.2026 |

---

**Erstellt:** 05.02.2026 10:00 von Marcel Zimmer

**Geändert:** 05.02.2026 14:30 von Marcel Zimmer

*Erstellt mit MZProtokoll...*
```

### Parser (`markdown_parsen`)

Der Parser ist ein zeilenbasierter Zustandsautomat mit dem internen Enum `Section`.
Beim Einlesen einer `## Überschrift` wechselt der Zustand:

```
Header → Protokollfuehrer → Teilnehmer → ZurKenntnis →
UeberMeeting → Status → Sicherheit → Eintraege
```

**Wichtig:** `|`-Zeichen in Zellen werden escaped (`\|`) gespeichert.
Die Funktion `tabellenzeile_aufteilen` verarbeitet dies beim Einlesen korrekt.

### Serialisierer (`markdown_erstellen`)

Baut den Markdown-String durch `String::push_str`-Aufrufe auf. Zeilenumbrüche in
Notizfeldern werden als ` <br> ` codiert, damit die Markdown-Tabelle einzeilig bleibt.

---

## PDF-Export

### Zweiphasen-Rendering

genpdf kennt die Gesamtseitenzahl erst nach dem Rendern. Um dennoch „Seite X von Y"
in die Fußzeile schreiben zu können, wird der Inhalt zweimal gerendert:

```
Durchlauf 1 (In-Memory-Puffer):
  → genpdf::SimplePageDecorator zählt Seiten mit
  → Ergebnis: gesamtseiten: usize

Durchlauf 2 (echte Datei):
  → FusszeileDekorator verwendet gesamtseiten
  → schreibt "Seite X von Y" unten rechts auf jede Seite
```

### `FusszeileDekorator`

Implementiert `genpdf::PageDecorator`. Die Fußzeile wird auf dem rohen Seitenbereich
platziert, bevor die Seitenränder gesetzt werden, damit sie im Randbereich liegt.

### `ZellenHintergrund<E>`

Da genpdf keine echte Tabellenformatierung mit Hintergrundfarben bietet, werden
TODO-Zeilen grau hinterlegt, indem sehr dichte horizontale Linien (0,15 mm Abstand,
Graustufe 220) gezeichnet werden. Weiße Zeilen verwenden Graustufe 255, um grauen
Überlauf der Vorgängerzeile abzudecken.

### Markdown-Links im PDF

Da genpdf keine Hyperlinks unterstützt, werden `[Text](URL)`-Links durch
`Text [N]` ersetzt und am Ende des Dokuments als nummiertes Linkverzeichnis gedruckt
(Funktion `markdown_links_extrahieren`).

### Schriftarten für PDF

Für den PDF-Export (`schrift_laden`) sucht die App nach Liberation Sans oder
Noto Sans in Familien-Verzeichnissen (über `genpdf::fonts::from_files`). Als
Fallback wird DejaVu Sans verwendet.
Wird keine Schrift gefunden, erscheint ein Fehlerdialog mit Installationshinweis.

---

## Theme-System

### Varianten

| Theme     | Beschreibung                                          |
|-----------|-------------------------------------------------------|
| `Hell`    | Helles egui-Standard-Theme                            |
| `Dunkel`  | Dunkles Theme, Hintergrund reines Schwarz             |
| `Omarchy` | Liest Farben aus `~/.config/omarchy/current/theme/colors.toml` |

### Omarchy-Integration

Die Funktion `omarchy_farben_laden` liest TOML-Zeilen der Form `key = "#rrggbb"` ein.
Verwendete Schlüssel:

| TOML-Schlüssel | Verwendung in der App              |
|----------------|------------------------------------|
| `background`   | Fensterhintergrund                 |
| `cursor`       | Farbe der inaktiven Schriften      |
| `accent`       | Buttons, Auswahl, Hover-Effekte    |
| `color3`       | Abschnittsbezeichnungen (Labels)   |
| `color2`       | Eingabetext in Textfeldern         |

Das `Omarchy`-Theme wird nur im Cycle angeboten, wenn die Konfigurationsdatei
gefunden wurde (`has_omarchy = true`).

---

## Datei-Dialoge und Thread-Kommunikation

Da `rfd::FileDialog` den Haupt-Thread blockieren würde, laufen alle Dialoge in
eigenen Threads. Die Kommunikation erfolgt über `std::sync::mpsc`:

```rust
let (sender, empfaenger) = mpsc::channel::<DialogErgebnis>();
self.dialog_rx = Some(empfaenger);
std::thread::spawn(move || {
    if let Some(pfad) = rfd::FileDialog::new()...pick_file() {
        let _ = sender.send(DialogErgebnis::Laden(pfad, inhalt));
    }
});
// Im nächsten update()-Aufruf:
if let Ok(ergebnis) = self.dialog_rx.try_recv() { ... }
```

Es kann immer nur ein Dialog gleichzeitig geöffnet sein (`dialog_rx` ist `Option`).

---

## Tastenkombinationen

| Kombination | Aktion                              |
|-------------|-------------------------------------|
| `Strg+N`    | Neues Protokoll (aktuelle Daten verwerfen) |
| `Strg+O`    | Datei öffnen (Markdown laden)       |
| `Strg+S`    | Speichern (Markdown)                |
| `Strg+P`    | PDF erzeugen (PDF-Export)           |
| `Strg+T`    | Theme wechseln                      |
| `Strg+W`    | Beenden (mit Bestätigungsdialog)    |
| `Strg+I`    | Über-Dialog öffnen                  |
| `Strg+H`    | Website öffnen (xdg-open)           |
| `↑`/`↓`     | Cursor zwischen Notizfeldern bewegen |

---

## Erweiterungsmöglichkeiten

### Neue Eintragsart hinzufügen

1. In `enum Art` eine neue Variante ergänzen.
2. In `Art::label()` einen Anzeigetext definieren.
3. In `Art::color()` eine Farbe zuweisen.
4. In `Art::all()` die Variante eintragen.
5. In `art_parsen()` den Text-Mapping-Eintrag ergänzen.
6. Ggf. in `pdf_inhalt_hinzufuegen` und im UI-Rendering behandeln.

### Neues exportiertes Feld hinzufügen

1. Feld in `ProtokollApp` als `String` oder passendem Typ hinzufügen.
2. In `markdown_erstellen` ausgeben.
3. In `markdown_parsen` einlesen (neuer `Section`-Zustand oder Header-Parsing).
4. In `pdf_inhalt_hinzufuegen` in die info_table-Zeile aufnehmen.
5. UI-Widget in `update()` ergänzen.

### Neue Schriftart unterstützen

In `new()` (für egui) und in `schrift_laden()` (für genpdf) den entsprechenden
Pfad in den jeweiligen Suchpfad-Arrays ergänzen.

### Omarchy-Farben erweitern

In `update()` im `Theme::Omarchy`-Zweig weitere `colors.get("key")`-Abfragen
und die entsprechenden `visuals`-Zuweisungen ergänzen.

---

## Build und Installation

### Voraussetzungen

- Rust (stable, getestet mit Edition 2021)
- Systemschrift: Liberation Sans oder Noto Sans (für PDF-Export)
- Linux: libxkbcommon, libwayland-dev (für eframe/Wayland)

### Debug-Build

```bash
cargo build
./target/debug/mzprotokoll
```

### Release-Build

```bash
cargo build --release
./target/release/mzprotokoll
```

### Linux-Installation (Systemweit)

```bash
chmod +x install.sh
./install.sh
```

Das Skript kopiert die Binary nach `/usr/local/bin/mzprotokoll` und die
`.desktop`-Datei nach `~/.local/share/applications/`.

---

*Diese README wurde am 25.02.2026 erstellt und beschreibt Version 1.0.0.*
