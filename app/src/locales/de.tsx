// German. Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Trace terms stay as loanwords: Trace, Traces, Tracing; the runtime is "Runtime".

import { dotted, num, plural } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "So groß wie das Spiel";
const FILES = { one: "Datei", other: "Dateien" };
const CHECKS = { one: "Prüfung", other: "Prüfungen" };

const de: Messages = {
  // App.tsx
  app: {
    address: (url) => `Adresse: ${url}. Verbindungsanleitung mit QR-Code anzeigen`,
    screens: "Bereiche",
    convert: "Konvertieren",
    active: (n) => `, ${num(n)} aktiv`,
    edit: "Bearbeiten",
    library: "Bibliothek",
    about: "Info",
    offline: "Offline, neuer Versuch…",
    stop: "Dump Forge beenden",
    stillRunning: (n) => (n === 1 ? "Ein Auftrag läuft noch" : `${num(n)} Aufträge laufen noch`),
    job: (id) => `Auftrag ${id}`,
    quitNote: (web, n) =>
      web
        ? `Das Beenden von Dump Forge stoppt ${n === 1 ? "ihn" : "sie"} und löscht die unfertige Ausgabe.`
        : `Das Schließen stoppt ${n === 1 ? "ihn" : "sie"} und löscht die unfertige Ausgabe.`,
    keepRunning: "Weiterlaufen lassen",
    stopAndQuit: (n) => (n === 1 ? "Auftrag stoppen und beenden" : "Aufträge stoppen und beenden"),
    stoppingJobs: "Aufträge werden gestoppt; unfertige Ausgaben werden gelöscht…",
    stoppingForge: "Dump Forge wird beendet…",
    couldntStop: "Dump Forge konnte nicht beendet werden",
    close: "Schließen",
    tryAgain: "Erneut versuchen",
    stopped: "Dump Forge wurde beendet",
    startAgain: (ps5) => (ps5 ? "Um fortzufahren, öffnen Sie die Kachel Dump Forge auf der PS5 erneut." : "Um fortzufahren, führen Sie denselben Befehl ps5-dump-forge serve erneut aus."),
    confirmStop: (web: boolean) => (web ? "Dump Forge beenden?" : "PS5 Dump Forge beenden?"),
    stopNote: (web, ps5) => (!web ? "Es läuft kein Auftrag." : ps5 ? "Zum erneuten Starten öffnen Sie die Kachel Dump Forge auf der PS5." : "Zum erneuten Starten führen Sie denselben Befehl ps5-dump-forge serve noch einmal aus."),
    quit: "Beenden",
    github: (name) => `${name} auf GitHub`,
    queued: "in der Warteschlange",
    progress: (percent, left) => `${percent} % fertig${left ? `, noch ${left}` : ""}`,
  },
  // Convert.tsx
  convert: {
    target: "Ziel",
    targetFormat: "Zielformat",
    alreadyFolder: "die Quelle ist bereits ein Ordner",
    lz4Backport:
      "Die Assets dieses Spiels sind LZ4-gepackt und sein LZ4-Manifest enthält den Backport: Entpacken Sie es zuerst im Tab „LZ4“ und entfernen Sie dann hier den Backport.",
    unreadable: "Erstellen nicht möglich: Diese Quelle konnte nicht gelesen werden (siehe „Quelle“).",
    noContentId: "Erstellen als fpkg nicht möglich: Die param.json dieses Spiels hat keine Content-ID, die ein Paket braucht.",
    blocked: (first, more) =>
      `Erstellen nicht möglich: ${first}${more > 0 ? ` (und ${num(more)} weitere, siehe „Quelle“)` : ""}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Dieses Spiel nutzt AMPR, hat aber kein ampr_emu: Die Konvertierung installiert das mitgelieferte ampr_emu ${version}.`;
      if (a === "trace") return `Die Konvertierung ersetzt die LZ4-Trace-Runtime durch das mitgelieferte Release-ampr_emu ${version}.`;
      if (a === "replace")
        return own
          ? `Die Konvertierung ersetzt das ampr_emu ${own} dieses Spiels durch das mitgelieferte ampr_emu ${version}.`
          : `Die Konvertierung ersetzt das ampr_emu dieses Spiels, das keine Version nennt, durch das mitgelieferte ampr_emu ${version}.`;
      return own
        ? `Das ampr_emu ${own} dieses Spiels ist so neu wie das mitgelieferte ${version} oder neuer: Die Konvertierung behält es.`
        : `Dieses Spiel hat bereits das mitgelieferte ampr_emu ${version}: Die Konvertierung behält es.`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "ampr_emu.index listet nicht die Dateien dieses Spiels" : state === "damaged" ? "ampr_emu.index ist nicht lesbar" : "Dieses Spiel hat keine ampr_emu.index"}: Die Konvertierung schreibt eine neue${traces ? " und lässt die alten Traces weg" : ""}.`,
    amprPkgIndex: (traces) =>
      `Das Paket erhält eine neue ampr_emu.index aus seinen Dateien${traces ? ", ohne die alten Traces" : ""}.`,
    removeBackport: "Backport entfernen",
    removeHint: (libs, emulators, other) =>
      `Lässt ${num(libs)} ${plural(libs, { one: "Backport-Bibliothek", other: "Backport-Bibliotheken" })} aus fakelib/ weg` +
      (emulators.length === 0
        ? other
          ? "; andere Homebrew-Bibliotheken bleiben."
          : "."
        : `; ${plural(emulators.length, { one: "der Emulator", other: "die Emulatoren" })} (${emulators.join(", ")})${other ? " und andere Homebrew-Bibliotheken" : ""} ${emulators.length === 1 && !other ? "bleibt" : "bleiben"}.`),
    cantRemove: (why) => `Backport kann nicht entfernt werden: ${why}`,
    compare: "Formate vergleichen",
    build: "Erstellen",
    likelyFail: (fmt) => (
      <>
        Als <b>{fmt}</b> wahrscheinlich nicht möglich: Das Format lehnt Dateinamen ab, die unter „Quelle“ aufgeführt sind.
      </>
    ),
    collides: "Ein Auftrag schreibt bereits diese Datei: Erneutes Erstellen würde fehlschlagen. Wählen Sie einen anderen Namen.",
    already: (fmt) => (
      <>
        Dieses Spiel wird bereits als <b>{fmt}</b> erstellt; erneutes Erstellen erzeugt eine zweite Kopie.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "Gut", ok: "Kompromiss", bad: "Nachteil" },
    columns: ["Tempo auf der Konsole", "Größe auf dem Datenträger"],
    rows: {
      pkg: {
        tagline: "Am besten, wenn Ihre Firmware es erlaubt",
        cells: ["Nativ, volles Tempo", "30–60 % kleiner, je nach Spiel"],
      },
      lz4: {
        tagline: "Experimentell, für Spiele mit AMPR",
        cells: ["30–40 % schnelleres Laden, laut den Autoren", "Etwa halb so groß wie das Spiel, laut den Autoren"],
      },
      ffpkg: { tagline: "Empfohlen für ShadowMountPlus", cells: ["Volles Image-Tempo", FLAT] },
      exfat: { tagline: "Kompatibilität", cells: ["Volles Image-Tempo", FLAT] },
      ffpfs: { tagline: "Experimentell", cells: ["Volles Image-Tempo", FLAT] },
      ffpfsc: {
        tagline: "Am kleinsten, experimentell",
        cells: ["150–250 MB/s, kann ruckeln", "Hängt von der Kompressionsstufe ab, meist 30–60 % kleiner"],
      },
      folder: { tagline: "Normale Spieldateien", cells: ["Volles Laufwerkstempo", FLAT] },
    },
    features: [
      ["Konvertieren", "zwischen Ordner, ffpkg, exfat, ffpfs, ffpfsc und fpkg, in beide Richtungen."],
      ["Bearbeiten", "der Dateien eines Spiels wie in einem Ordner, dazu Cover, Firmware, Backport, DLC und Probleme mit Dateinamen vor dem Erstellen."],
      ["Löschen", "eines Spiels vom Datenträger, nach Ihrer Bestätigung."],
      ["LZ4", "Tracing eines AMPR-Spiels, dann Packen seiner Assets mit intelligenter Reparatur (experimentell)."],
      ["Gestreamt", "von jeder Quelle zu jedem Ziel: ohne temporäre Kopie, auch für das Image in einem ffpfsc oder ein Spiel in einem Archiv."],
      ["Archive", "ein .zip, .7z oder .rar mit einem Spiel wird wie ein Ordner durchsucht, konvertiert oder gepackt."],
      ["Bibliothek", "die Spiele auf diesem Computer oder dieser Konsole, bereit zum Konvertieren und Bearbeiten."],
      ["Sicher", "Ihr Spiel wird nie verändert, außer Sie wünschen es; Müll-Dateien werden übersprungen, ungültige Dateinamen vorab gemeldet."],
      ["USB und SD-Karten", "geschrieben auf Sticks und Karten mit exFAT oder FAT32."],
      ["An Ort und Stelle oder neu gebaut", "Änderungen gehen direkt ins Image, wenn sie passen, sonst in ein neues Image, das es nach der Prüfung ersetzt."],
    ],
    howToConnect: "Verbindung herstellen",
    connectText: (url) => (
      <>Öffnen Sie {url} in einem Browser auf einem beliebigen Gerät in Ihrem lokalen Netzwerk oder scannen Sie diesen Code.</>
    ),
    close: "Schließen",
    about: "Info",
    aboutText: "Dieses Tool konvertiert ein Spiel von jedem Format in jedes andere, auf jeder Plattform, auch auf der PS5.",
    connectAny: "Verbindung von jedem Gerät",
    useAddress: (url) => <>Öffnen Sie {url} oder scannen Sie diesen Code.</>,
    compare: "Formate vergleichen",
    compareLabel: "Formatvergleich",
    format: "Format",
    upstream: (app) =>
      `Angaben der Autoren: was die Autoren von ampr_emu und Lazy_AMPR berichten, nicht von ${app} gemessen; die Werte schwanken je nach Spiel. ampr_emu 0.4.2.1 ist ein Test-Build der Autoren; bekanntes Problem: Manche Spiele stürzen beim Speichern ab.`,
    bundled: "LZ4-gepackte Ordner laufen mit ampr_emu 0.4.2.1 von drakmor (GPL-3.0), unverändert mitgeliefert. Quellcode:",
    lazy: "Der Ablauf „erst Tracing, dann Packen“ folgt Lazy_AMPR von Nazky:",
    unrar:
      ".rar-Archive werden mit der mitgelieferten UnRAR-Bibliothek von Alexander Roshal gelesen (nur Extrahieren, unter der UnRAR-Lizenz).",
    language: "Sprache",
    system: "System",
  },
  // Edit.tsx
  edit: {
    details: "Details",
    deleting: "Quelle wird gelöscht…",
    delete: "Quelle löschen",
    deleted: "Gelöscht:",
    filter: "Nach Pfad filtern",
    filterLabel: "Dateien nach Pfad filtern",
    firstOf: (max, n) => plural(max, {
  one: `erste Datei von ${num(n)}`,
  other: `erste ${num(max)} von ${num(n)}`,
}),
    fileCount: (n) => `${num(n)} ${plural(n, FILES)}`,
    noMatch: (f) => `Keine Dateien passen zu „${f}“.`,
    noListing: "Die Dateiliste des Archivs wird nicht unterstützt.",
    noParam: "Keine sce_sys/param.json.",
    reader: "Lesemodul",
    leftovers: "Übrig gebliebene Dateien und Ordner",
    found: (n) => `${num(n)} gefunden`,
    leftoversNote: (app) =>
      `Unfertige Ausgaben, meist von abgebrochenen Aufträgen. Entfernen Sie nur die, die Sie kennen, und nur, wenn kein Auftrag läuft; ${app} löscht sie nie selbst.`,
    checked: (n) => `Geprüfte Ordner (${num(n)})`,
    /** Edit's folds under the game card. */
    facts: "Spieldaten",
    explorer: "Spieldateien",
    howTitle: "So funktioniert es",
    /** The empty state's paragraphs. */
    how: (app) => [
      "Öffnen Sie einen **Spielordner** oder ein **Image** und ändern Sie es wie einen Ordner auf einem USB-Stick: **Dateien hinzufügen und ersetzen** (hineinziehen oder „Hinzufügen“ verwenden), **umbenennen, löschen**, Ordner anlegen.",
      `Nichts ändert sich, **bis Sie auf „Anwenden“ klicken**: Ihre Änderungen sammeln sich hier, dann schreibt ${app} sie in einem Durchgang direkt in das Image oder den Ordner und prüft das Ergebnis. Es ist **kein eingebundenes Laufwerk**: ${app} merkt sich Ihre Änderungen und wendet sie selbst an.`,
      `Passen die Änderungen hinein, gehen sie **direkt in das Image**; wenn nicht (kein Platz mehr darin oder ein Format, das sie nicht aufnehmen kann), erstellt ${app} daneben ein neues Image mit Ihren Änderungen und setzt es an seine Stelle, sobald es die Prüfung besteht. In beiden Fällen bleibt das Image bei einem Fehler, wie es war.`,
      "Archive können durchsucht, aber nicht bearbeitet werden.",
    ],
    location: "Ort im Spiel",
    root: "Spiel",
    up: "Nach oben",
    upTip: "Einen Ordner nach oben",
    name: "Name",
    size: "Größe",
    items: (n) => `${num(n)} ${plural(n, { one: "Element", other: "Elemente" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "Ordner", other: "Ordner" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "Treffer", other: "Treffer" })}`,
    emptyFolder: "Dieser Ordner ist leer.",
    open: (name) => `${name} öffnen`,
    archiveNote: "Archive können durchsucht, aber nicht bearbeitet werden. Um es zu ändern, konvertieren Sie es zuerst in einen Ordner oder ein Image.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "Dateien hinzufügen",
    addFolder: "Ordner hinzufügen",
    pickAddFiles: "Dateien zum Hinzufügen wählen",
    pickAddFolder: "Ordner zum Hinzufügen wählen",
    nameLabel: "Name",
    renameItem: (name) => `${name} umbenennen`,
    removeItem: (name) => `${name} löschen`,
    renameTip: "Umbenennen (F2)",
    removeTip: "Löschen (Entf-Taste)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "Extrahieren",
    extractItem: (name: string) => `${name} extrahieren`,
    extractTip: "In einen gewählten Ordner kopieren",
    extractFolder: "Ordner extrahieren",
    extractFolderTip: "Den angezeigten Ordner in einen gewählten Ordner kopieren",
    extractGameTip: "Das ganze Spiel in einen gewählten Ordner kopieren",
    pickExtract: "Zielordner zum Extrahieren wählen",
    extractPending: "Extrahiert die Dateien im jetzigen Zustand, ohne Ihre ausstehenden Änderungen",
    nameEmpty: "Geben Sie einen Namen ein.",
    nameBad: "Ein Name darf weder / noch \\ enthalten und nicht . oder .. sein",
    nameTaken: (name) => `${name} gibt es hier schon.`,
    markNew: "Neu",
    markReplaced: "Ersetzt",
    markRenamed: "Umbenannt",
    wasAt: (path) => `Vorher ${path}`,
    sizeLater: "Mehr, als aufgelistet werden konnte: Der Rest wird gelesen, wenn die Änderungen angewendet werden",
    dropInto: (folder) => `Ablegen, um zu ${folder} hinzuzufügen`,
    pending: (n) =>
      plural(n, { one: `${num(n)} Änderung noch nicht angewendet`, other: `${num(n)} Änderungen noch nicht angewendet` }),
    undo: "Rückgängig",
    undoTip: "Letzte Änderung rückgängig machen",
    discard: "Alle verwerfen",
    discardTip: "Alle noch nicht angewendeten Änderungen vergessen",
    discardTitle: (n) =>
      plural(n, { one: `${num(n)} Änderung verwerfen?`, other: `Alle ${num(n)} Änderungen verwerfen?` }),
    discardText: "Sie werden vergessen, und am Spiel ändert sich nichts. Das lässt sich nicht rückgängig machen.",
    discardNo: "Nein",
    changesList: "Änderungen der Reihe nach",
    willNotApply: (n) =>
      plural(n, {
        one: `${num(n)} Änderung lässt sich so nicht anwenden. Machen Sie sie rückgängig oder verwerfen Sie alle.`,
        other: `${num(n)} Änderungen lassen sich so nicht anwenden. Machen Sie sie rückgängig oder verwerfen Sie alle.`,
      }),
    opAdd: (to) => <>{to} hinzufügen</>,
    opAddFolder: (to) => <>Ordner {to} hinzufügen</>,
    opDelete: (path) => <>{path} löschen</>,
    opRename: (from, to) => <>{from} in {to} umbenennen</>,
    opMkdir: (path) => <>Neuer Ordner {path}</>,
    opFrom: (src) => <>Aus {src}</>,
    whyInvalid: (path) => <>{path} ist kein gültiger Pfad im Spiel.</>,
    whyMissing: (path) => <>{path} ist nicht im Spiel.</>,
    whyExists: (path) => <>{path} gibt es schon.</>,
    whyFile: (path) => <>{path} ist eine Datei.</>,
    whyFolder: (path) => <>{path} ist ein Ordner.</>,
    whyInto: (path) => <>{path} kann nicht in sich selbst verschoben werden.</>,
    conflictTitle: (n) =>
      plural(n, { one: `${num(n)} Element gibt es hier schon`, other: `${num(n)} Elemente gibt es hier schon` }),
    conflictIn: (folder) => <>In {folder}:</>,
    conflictText:
      "„Ersetzen“ setzt die neuen an ihre Stelle: Sobald die Änderungen angewendet sind, ist der alte Inhalt endgültig weg, und eine fehlerhafte Datei kann verhindern, dass das Spiel startet. Ein Ordner, der auf einen Ordner kommt, wird zusammengeführt; seine Dateien mit gleichen Namen werden ersetzt.",
    replace: "Ersetzen",
    skip: "Überspringen",
    cancel: "Abbrechen",
    /** How the changes go in (the pending bar), from core's check before Apply. */
    modeLabel: "Art der Anwendung",
    checking: "Prüfe, wie sich die Änderungen anwenden lassen",
    checkFailed: "Die Änderungen konnten nicht geprüft werden:",
    checkAgain: "Erneut prüfen",
    inPlace: "Direkt",
    rebuild: "Neu erstellen",
    rebuildNeeded: "Neu erstellen nötig",
    cantApply: "Nicht anwendbar",
    changedDirectly: "Direkt geändert",
    folderText: "Wird direkt in den Ordner geschrieben.",
    inPlaceText: "Wird in das Image geschrieben.",
    inPlaceNeeds: (need: string, free: string) => `Wird in das Image geschrieben; braucht ${need} der ${free}, die darin frei sind.`,
    noRoom: (need: string, free: string) => `Die Änderungen brauchen ${need}, im Image sind ${free} frei.`,
    alwaysRebuilt: "Images im Format ffpfsc und fpkg werden immer neu erstellt.",
    rebuildText: "Daneben wird ein neues Image geschrieben, das es nach der Prüfung ersetzt.",
    rebuildNeeds: (need: string, free: string) =>
      `Daneben wird ein neues Image geschrieben (etwa ${need} freier Platz nötig, ${free} frei), das es nach der Prüfung ersetzt.`,
    uploadLater: (n: number) =>
      plural(n, {
        one: `${num(n)} Datei aus diesem Browser wird hochgeladen, wenn Sie auf „Anwenden“ klicken, und dann geprüft.`,
        other: `${num(n)} Dateien aus diesem Browser werden hochgeladen, wenn Sie auf „Anwenden“ klicken, und dann geprüft.`,
      }),
    decidedOnApply: (n: number) =>
      plural(n, {
        one: `Andere Änderungen bauen auf der Datei aus diesem Browser auf; wie sie angewendet werden, entscheidet sich daher erst nach dem Hochladen, wenn Sie auf „Anwenden“ klicken.`,
        other: `Andere Änderungen bauen auf den ${num(n)} Dateien aus diesem Browser auf; wie sie angewendet werden, entscheidet sich daher erst nach dem Hochladen, wenn Sie auf „Anwenden“ klicken.`,
      }),
    sourceLocked: "Auf dieses Spiel werden gerade Änderungen angewendet. Wählen Sie eine andere Quelle, wenn der Auftrag beendet ist.",
    notOpened: (path) => <>{path} wurde nicht geöffnet: Auf dieses Spiel werden gerade Änderungen angewendet. Öffnen Sie es erneut, wenn der Auftrag beendet ist.</>,
    apply: "Anwenden",
    applyTip: "Alle hier aufgeführten Änderungen anwenden",
    applyTitle: (n: number) => plural(n, { one: `${num(n)} Änderung anwenden?`, other: `${num(n)} Änderungen anwenden?` }),
    applyInPlace: (name) => <>Sie werden direkt in {name} geschrieben. Danach lässt sich nichts rückgängig machen.</>,
    applyRebuild: (name) => (
      <>Neben {name} wird ein neues Image mit Ihren Änderungen geschrieben, geprüft und dann an seine Stelle gesetzt. Sobald es das Image ersetzt hat, lässt sich nichts rückgängig machen.</>
    ),
    closeImage: "Läuft das Spiel aus diesem Image, schließen Sie es zuerst.",
    closeFolder: "Läuft das Spiel aus diesem Ordner, schließen Sie es zuerst.",
    applyRestores: "Eine frühere Bearbeitung dieses Images wurde unterbrochen: Das Image wird zuerst wiederhergestellt, wie es war.",
    uploading: (i: number, n: number, sent: string, total: string) => `Datei ${num(i)} von ${num(n)} wird hochgeladen: ${sent} von ${total}`,
    uploadLabel: "Hochladen",
    uploadCancelled: "Hochladen abgebrochen. Am Spiel hat sich nichts geändert.",
    applyRunning: "Die Änderungen werden angewendet. Bearbeiten ist wieder möglich, wenn der Auftrag unten beendet ist.",
    applyFailed: "Die Änderungen wurden nicht angewendet:",
    applyRefused: "Die Änderungen lassen sich so nicht anwenden:",
    applyCancelled: "Der Auftrag wurde abgebrochen. Ihre Änderungen stehen weiterhin hier.",
    folderPartly: "Ein Teil davon ist vielleicht schon im Ordner: Die angezeigten Dateien sind sein aktueller Inhalt.",
    interrupted: "Eine frühere Bearbeitung dieses Images wurde unterbrochen. Das Image wird wiederhergestellt, wie es war, bevor Ihre nächsten Änderungen geprüft oder angewendet werden.",
  },
  // JobList.tsx
  jobs: {
    jobs: "Aufträge",
    pending: (n) => `${num(n)} ${plural(n, { one: "läuft oder wartet", other: "laufen oder warten" })}`,
    allFinished: "alle abgeschlossen",
    stages: {
      scan: "Quelle wird gelesen",
      preflight: "Vorabprüfung",
      check: "Prüfung",
      plan: "Planung",
      compress: "Komprimierung",
      write: "Schreiben",
      measure: "Pakete werden vermessen",
      pack: "Packen",
      verify: "Prüfen",
      finalize: "Abschluss",
    },
    steps: {
      scan: "Lesen",
      preflight: "Prüfung",
      check: "Prüfung",
      plan: "Planung",
      compress: "Komprimierung",
      write: "Schreiben",
      measure: "Vermessen",
      pack: "Packen",
      verify: "Verifizieren",
      finalize: "Abschluss",
    },
    stepsLabel: "Schritte",
    stepDone: (s) => `${s}: erledigt`,
    stepLater: (s) => `${s}: noch nicht begonnen`,
    packingWriting: "Packen und Schreiben",
    rate: { read: "Lesen", compress: "Komprimieren", pack: "Packen", write: "Schreiben" },
    showPath: "Pfad anzeigen",
    showFinder: "Im Finder zeigen",
    showExplorer: "Im Explorer zeigen",
    showFolder: "Im Ordner zeigen",
    lz4: {
      pack: "LZ4 packen",
      unpack: "LZ4 entpacken",
      trace: "LZ4 Trace",
      unpatch: "LZ4 Patch entfernen",
      runtime: "AMPR-Runtime",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "Bearbeiten",
    editRebuild: "Bearbeiten · neu erstellt",
    extract: "Extrahieren",
    job: (id) => `Auftrag ${id}`,
    done: "Fertig",
    cancelled: "Abgebrochen",
    failed: "Fehlgeschlagen",
    cancelling: "Wird abgebrochen…",
    queued: "Wartet",
    dismiss: "Ausblenden",
    stopJob: "Stoppen?",
    cancel: "Abbrechen",
    stopHint: "Klicken Sie innerhalb von 4 Sekunden erneut auf „Stoppen?“, um den Auftrag zu stoppen.",
    from: "aus ",
    left: (d) => `noch ${d}`,
    inStep: (time) => `${time} in diesem Schritt`,
    result: (bytes, n) => `${bytes}, ${num(n)} ${plural(n, FILES)} →`,
    outputPath: "Ausgabepfad",
    checks: "Prüfungen",
    verifiedPlain: (n) => `Geprüft: ${num(n)} ${plural(n, CHECKS)} bestanden`,
    verifiedFull: (n) => `Vollständige Prüfung bestanden: ${num(n)} ${plural(n, CHECKS)}`,
    verifiedFast: (n) => `Schnelle Prüfung bestanden: ${num(n)} ${plural(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `${checked} von ${total} geprüft, in ${num(n)} ${plural(n, { one: "Stichprobe", other: "Stichproben" })}`,
    cantBuild: "Dieses Spiel kann nicht erstellt werden:",
    cantEdit: "Die Änderungen können nicht angewendet werden:",
    cantExtract: "Extrahieren nicht möglich:",
    cantBuildAs: (f) => (
      <>
        Dieses Spiel kann nicht als <b>{f}</b> erstellt werden:
      </>
    ),
    log: (n) => `Log (${num(n)})`,
    logLabel: "Log",
  },
  // Library.tsx
  library: {
    pickTitle: "Ordner für die Bibliothek wählen",
    scanningIn: (name) => `${name} wird durchsucht`,
    scanning: "Wird durchsucht",
    cantScanFolder: "Dieser Ordner kann nicht durchsucht werden",
    chooseAnother: "Anderen Ordner wählen",
    pickPrompt: "Wählen Sie einen Ordner mit Ihrer Bibliothek zum Durchsuchen",
    choose: "Ordner wählen",
    cantScan: "Die Bibliothek kann nicht durchsucht werden:",
    cantDelete: "Löschen nicht möglich:",
    deleted: (path) => `Gelöscht: ${path}`,
    truncated: "Suchlimit erreicht. Diese Liste ist eventuell unvollständig.",
    limits: "Die Suche geht höchstens sechs Ordnerebenen tief und endet nach etwa 15 Sekunden.",
    noRoots: "Keine Ordner zum Durchsuchen verfügbar.",
    none: "Keine Spiele gefunden. Die Suche geht bis zu sechs Ebenen tief und überspringt Systemordner. Andere Quellen können Sie weiterhin unter „Bearbeiten“ oder „Konvertieren“ wählen.",
    skipped: "Übersprungene Orte und Dateien",
    stays: "Die Ergebnisse bleiben hier, bis Sie „Aktualisieren“ drücken. Neue Ausgaben erscheinen nach dem Aktualisieren.",
    library: "Bibliothek",
    storageTip: "Freier Speicher im internen Speicher der Konsole (/data), Stand der letzten Suche",
    free: (free, total) => (
      <>
        <b>{free}</b> von {total} frei
      </>
    ),
    refresh: "Aktualisieren",
    cards: "Kachelansicht",
    list: "Listenansicht",
    games: "Spiele",
    edit: (n) => `${n} bearbeiten`,
    convert: (n) => `${n} konvertieren`,
    remove: (n) => `${n} löschen`,
    lz4Tip: "LZ4-gepackt: ampr_emu-Asset-Pakete",
    lz4Packed: " gepackt",
  },
  // Lz4.tsx
  lz4: {
    title: "LZ4-Asset-Pakete",
    help: "So funktioniert LZ4",
    scenario: "LZ4-Szenario",
    scenarios: { trace: "Trace", pack: "Packen/Entpacken" },
    actions: { patch: "Patchen", unpatch: "Patch entfernen", pack: "Packen", unpack: "Entpacken" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "Zeichnet beim Spielen auf, welche Assets das Spiel liest, damit „Packen“ das nutzen kann."
      : packed
        ? "Schreibt die LZ4-Pakete des Spiels als normale Dateien zurück."
        : "Packt die Assets des Spiels in LZ4-Volumes, die ampr_emu während des Spiels liest.",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "Erstellt dieses Image mit der Trace-Runtime neu und ersetzt es dann."
        : "Installiert die Trace-Runtime in diesem Ordner."
      : image
        ? "Erstellt dieses Image mit der Release-Runtime neu und ersetzt es dann."
        : "Stellt die Release-Runtime in diesem Ordner wieder her.",
    noAmpr: "das Spiel nutzt kein AMPR",
    noEboot: "es gibt kein eboot.bin",
    packedFirst: "die Quelle ist LZ4-gepackt: zuerst entpacken",
    damaged: "seine LZ4-Pakete sind beschädigt",
    archive: "für ein Archiv ist kein Tracing möglich: zuerst konvertieren",
    chooseFirst: "zuerst eine Quelle wählen",
    notRead: "die Quelle ist noch nicht gelesen",
    pickProfile: "LZ4-Profil",
    profileFilter: "LZ4-Profil (TOML)",
    pickTracesFolder: "LZ4-Traces (ein Ordner mit ampr_commands.bin und ampr_emu.index)",
    pickTracesEither: "LZ4-Traces: die zip oder ein Ordner mit beiden Dateien",
    pickTracesZip: "LZ4-Traces (zip)",
    unreadable: "Start nicht möglich: Diese Quelle konnte nicht gelesen werden (siehe „Quelle“).",
    pending: (a) => a === "patch"
      ? "Patchen ist nicht möglich, solange Aufträge laufen oder warten."
      : "Der Patch kann nicht entfernt werden, solange Aufträge laufen oder warten.",
    replacing: "Ein Auftrag erstellt dieses Image bereits neu: Warten Sie, bis er fertig ist.",
    blocked: (a, first, more) => {
      const rest = `${first}${more > 0 ? ` (und ${num(more)} weitere, siehe „Quelle“)` : ""}.`;
      return a === "patch"
        ? `Patchen nicht möglich: ${rest}`
        : a === "unpatch"
          ? `Patch kann nicht entfernt werden: ${rest}`
          : a === "pack"
            ? `Packen nicht möglich: ${rest}`
            : `Entpacken nicht möglich: ${rest}`;
    },
    chooseGame: "Wählen Sie zuerst einen Spielordner oder ein Image.",
    noAmprGame: "Dieses Spiel nutzt kein AMPR, daher ist LZ4 hier nicht nutzbar.",
    additionalContent: "Das ist ein Zusatzinhalt (ohne eboot.bin), daher ist LZ4 hier nicht nutzbar.",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `Dieses Spiel hat kein ampr_emu: „Packen“ installiert das mitgelieferte ampr_emu ${version}.`
        : a === "patch"
          ? `Dieses Spiel hat kein ampr_emu: „Patchen“ installiert die mitgelieferte Trace-Runtime (ampr_emu ${version}).`
          : `Dieses Spiel hat kein ampr_emu: „Patch entfernen“ installiert die mitgelieferte Release-Runtime (ampr_emu ${version}).`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "ampr_emu.index listet nicht die Dateien dieses Spiels" : state === "damaged" ? "ampr_emu.index ist nicht lesbar" : "Dieses Spiel hat keine ampr_emu.index"}: „${a === "pack" ? "Packen" : a === "patch" ? "Patchen" : "Patch entfernen"}“ schreibt eine neue.`,
    unavailable: (items) => `Nicht für diese Quelle: ${items.join("; ")}.`,
    readOnly: (kind) => <>
      <b>{kind}</b> ist auf der Konsole schreibgeschützt, daher ist kein Tracing möglich. Konvertieren Sie es zuerst in <b>ffpkg</b> oder <b>exfat</b>.
    </>,
    action: "Aktion",
    traceAction: "Trace-Aktion",
    changes: (a, image) => a === "patch"
      ? `Ändert ${image ? "dieses Image" : "diesen Ordner"}: Die Trace-Runtime ersetzt fakelib/libSceAmpr.sprx, ohne Sicherung.`
      : `Ändert ${image ? "dieses Image" : "diesen Ordner"}: Die Release-Runtime kommt zurück und die Traces werden gelöscht, ohne Sicherung.`,
    replay: (web) => web
      ? "Spielen Sie und wählen Sie diese Quelle dann erneut, um ihre Traces zu nutzen. „Trace“ bietet sie auch als zip an."
      : "Spielen Sie und wählen Sie diese Quelle dann erneut, um ihre Traces zu nutzen.",
    room: "Platz für den Trace",
    roomHint: "Freier Platz für den Trace des Spiels. Binden Sie das Image mit image_rw= ein, sonst wird nichts aufgezeichnet.",
    alreadyRelease: "Bereits mit der Release-Runtime: Nur der Index wird neu geschrieben.",
    spaceHint: "Braucht freien Platz etwa in Größe des Images; das Image wird erst ersetzt, wenn die Kopie geprüft ist.",
    deletesTraces: (a, bytes, web) =>
      a === "patch"
        ? `Patchen löscht die Traces dieser Quelle (${bytes}): ${web ? "Laden Sie sie zuerst herunter." : "Kopieren Sie sie zuerst."}`
        : `Das Entfernen des Patches löscht die Traces dieser Quelle (${bytes}): ${web ? "Laden Sie sie zuerst herunter." : "Kopieren Sie sie zuerst."}`,
    useProfile: "Stattdessen ein Regelprofil verwenden",
    profile: "Profil",
    choose: "Wählen",
    profileHint: "Eine TOML-Datei mit Packregeln, die statt Traces verwendet wird.",
    target: "Ziel",
    packInto: "Packen in",
    direct: "Packt direkt in das Image; die Dateien des Spiels werden zweimal gelesen.",
    targetFormat: "Zielformat",
    unpackFolder: "Ein neuer Spielordner, in dem die Pakete als normale Dateien zurückgeschrieben sind.",
    unpackHint: (n) =>
      `${n === null ? "Schreibt die gepackten Dateien als normale Dateien zurück" : `Schreibt ${plural(n, { one: `${num(n)} gepackte Datei als normale Datei`, other: `${num(n)} gepackte Dateien als normale Dateien` })} zurück`}.`,
    likelyFail: (fmt) => <>
      Als <b>{fmt}</b> wahrscheinlich nicht möglich: Das Format lehnt Dateinamen ab, die unter „Quelle“ aufgeführt sind.
    </>,
    collides: (a, folder) => {
      const what = folder ? "diesen Ordner" : "diese Datei";
      return a === "pack"
        ? `Ein Auftrag schreibt bereits ${what}: Packen braucht einen anderen Namen.`
        : `Ein Auftrag schreibt bereits ${what}: Entpacken braucht einen anderen Namen.`;
    },
    already: (a, fmt) => a === "pack"
      ? <>
        Dieses Spiel wird bereits als <b>{fmt}</b> geschrieben; erneutes Packen erzeugt eine zweite Kopie.
      </>
      : <>
        Dieses Spiel wird bereits als <b>{fmt}</b> geschrieben; erneutes Entpacken erzeugt eine zweite Kopie.
      </>,
    profileRules: "Packt, was die Regeln des Profils vorgeben; Dateien, die die Engine direkt öffnet, bleiben ungepackt.",
    chosenRules: "Packt die Dateien, die das Spiel in diesen Traces gelesen hat; Dateien, die die Engine direkt öffnet, bleiben ungepackt.",
    ownRules: "Packt die Dateien, die das Spiel in seiner letzten Trace-Sitzung gelesen hat; Dateien, die die Engine direkt öffnet, bleiben ungepackt.",
    guessRules: "Keine Traces: Gepackt wird nach einer weniger zuverlässigen eingebauten Schätzung. Zeichnen Sie zuerst Traces auf oder wählen Sie Traces oder ein Profil.",
    recorded: "Aufgezeichnete Traces",
    recordedHint: (bytes) => `ampr_commands.bin (${bytes}): „Packen“ nutzt diese Traces.`,
    downloadHint: "Schließen Sie das Spiel, warten Sie eine Minute und laden Sie dann im Browser eines Computers herunter. Wählen Sie die zip dort unter „Packen/Entpacken“ → „Traces“.",
    download: (bytes, zip) => `Traces (${bytes}) als ${zip} herunterladen`,
    missingIndex: "Keine ampr_emu.index in dieser Quelle: „Packen“ braucht sie neben ampr_commands.bin.",
    truncatedJournal: "Ein am Ende abgeschnittenes Journal lässt sich trotzdem packen, bis zum letzten vollständigen Eintrag.",
    fakelibWarning: (recorded) => recorded
      ? "Wenige oder keine Traces? Schalten Sie den fakelib-Updater aus, patchen Sie erneut und spielen Sie noch einmal."
      : "Schalten Sie beim Tracing den fakelib-Updater aus, sonst wird nichts aufgezeichnet.",
    emptyTitle: "Wählen Sie ein Spiel mit AMPR",
    experimental: "Experimentell: Bewahren Sie eine Kopie des Originalspiels auf.",
    chooseSource: "Quelle wählen",
    source: "Quelle",
    traces: "Traces",
    optionalTraces: "Traces (optional)",
    unlock: "Entsperren, um andere Traces zu wählen",
    loaded: "Traces sind bereits aus dieser Quelle geladen",
    lockedHint: (bytes) => `Die eigenen Traces dieser Quelle (${bytes}). Klicken Sie auf das Schloss, um andere zu wählen.`,
    clear: "Leeren",
    zip: "Zip",
    folder: "Ordner",
    tracesHint: "Wählen Sie die amprtrace-zip aus „Traces herunterladen“ oder einen Ordner mit beiden Trace-Dateien.",
    useOwn: "Traces der Quelle verwenden",
    patchResult: (a, n) => a === "patch"
      ? `Gepatcht: Die Trace-Runtime ist installiert; der Index listet ${num(n)} ${plural(n, FILES)}.`
      : `Patch entfernt: Die Release-Runtime ist zurück; der Index listet ${num(n)} ${plural(n, FILES)}.`,
    removed: (files) => `${plural(files.length, { one: "Alte Trace-Datei entfernt", other: "Alte Trace-Dateien entfernt" })}: ${files.join(", ")}.`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "So funktioniert LZ4",
    close: "Schließen",
    warning: "Experimentell, nur für AMPR-Spiele. Bewahren Sie eine Kopie des Originalspiels auf.",    body: () => (
      <>
        <h3>1. Packen/Entpacken</h3>
        <ol>
          <li>
            Wählen Sie das Originalspiel, <b>Packen/Entpacken</b> und die Traces (oder ein Profil).
          </li>
          <li>
            Schreiben Sie einen gepackten Ordner oder ein einfaches Image; ein Ordner lässt sich unter <b>Konvertieren</b> in
            jedes Format umwandeln.
          </li>
          <li>
            Bei einer gepackten Quelle erscheint stattdessen <b>Entpacken</b>: Die ungepackten Dateien kommen zurück.
          </li>
          <li>Packen behält das ampr_emu des Spiels, wenn es die mitgelieferte Version oder neuer ist und LZ4-Pakete lesen kann; sonst installiert es das mitgelieferte Release-ampr_emu, auch anstelle der LZ4-Trace-Runtime und in einem Spiel, das keines hat.</li>
        </ol>

        <h3>2. Trace</h3>
        <ol>
          <li>
            Wählen Sie das <b>ffpkg</b> oder <b>exfat</b> des Spiels, dann <b>Trace</b> → <b>Patchen</b>.
          </li>
          <li>Schalten Sie den fakelib-Updater aus: Er überschreibt die Trace-Runtime.</li>
          <li>
            Binden Sie es mit <span className="path">image_rw=</span> ein, spielen Sie eine Sitzung, schließen Sie das Spiel.
          </li>
          <li>
            Klicken Sie auf der PS5-Seite auf <b>Traces herunterladen</b> oder kopieren Sie ampr_commands.bin und ampr_emu.index.
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    drives: "Laufwerke",
    location: "Ort",
    parent: "Übergeordneter Ordner",
    allDrives: "Alle Laufwerke",
    truncated: (n) => plural(n, {
  one: "Nur der erste Eintrag wird angezeigt.",
  other: `Nur die ersten ${num(n)} Einträge werden angezeigt.`,
}),
    loading: "Wird geladen",
    noDrives: "Keine Laufwerke gefunden.",
    noFolders: "Hier gibt es keine Ordner.",
    noImages: "Hier gibt es keine Ordner, PS5-Images oder Archive.",
    noFiles: (ext) => `Hier gibt es keine Ordner oder ${dotted(ext, "- oder ")}-Dateien.`,
    chooseFiles: (n) => `${num(n)} ${plural(n, FILES)} wählen`,
    nothing: "Hier gibt es keine Ordner oder Dateien.",
    cancel: "Abbrechen",
    chooseFolder: "Diesen Ordner wählen",
  },
  // common.tsx
  common: {
    folder: "Ordner",
    pickFolder: "Spielordner",
    pickImage: "Spiel-Image oder -Archiv",
    pickImageFilter: "Images und Archive",
    pickFolderButton: "Ordner",
    pickImageButton: "Image/Archiv",
    source: "Quelle",
    emptyTitle: "Wählen Sie einen Spielordner, ein Image oder ein Archiv",
    emptyText: "Ein PS5-Spielordner oder ein Image, Paket bzw. Archiv davon.",
    accepted: "Unterstützte Quellen",
    unknownTitle: "Unbekannter Titel",
    additionalContent: "Zusatzinhalt",
    reading: "Quelle wird gelesen",
    unreadable: "Quelle nicht lesbar",
    backport: "Backport",
    emulators: "Emulatoren",
    declares: (fw) => `gibt ${fw} an`,
    fwUnknown: "Firmware anhand der ausführbaren Dateien nicht ermittelbar (kein lesbares SDK)",
    emulatorList: (names) => `Emulatoren: ${names}`,
    otherEmulator: "Andere",
    sdk: (v) => `SDK ${v} (angegeben)`,
    titleId: "Titel-ID",
    version: "Version",
    firmware: "Firmware",
    fwDeclared: "angegeben",
    fwBackport: "Backport",
    fwExecutables: "ausführbare Dateien",
    size: "Größe",
    files: (n) => `${num(n)} ${plural(n, FILES)}`,
    contentId: "Content-ID",
    noAmpr: "eboot.bin importiert libSceAmpr nicht",
    runtime: {
      forge_release: "Forge-Release",
      forge_trace: "Forge-Trace",
      other: "andere",
      none: "keine",
    },
    packed: "Gepackt",
    damaged: "Beschädigte Pakete",
    traced: "Mit Traces",
    plain: "Ungepackt",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} in ${num(volumes)} ${plural(volumes, { one: "Volume", other: "Volumes" })}` +
      (percent === null ? "" : `, ${num(percent)} % der Größe`),
    journal: (size) => `Journal ${size}`,
    noJournal: "noch kein Journal",
    runtimeIs: (label) => `Runtime: ${label}`,
    index: {
      stale: "ampr_emu.index veraltet: Eine Konvertierung erneuert sie",
      damaged: "ampr_emu.index beschädigt: Eine Konvertierung schreibt sie neu",
      missing: "keine ampr_emu.index: Eine Konvertierung schreibt eine",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `eine Konvertierung installiert das mitgelieferte ampr_emu ${version}`
        : a === "replace"
          ? `eine Konvertierung ersetzt die Runtime durch das mitgelieferte ampr_emu ${version}`
          : `ampr_emu ${version}: Eine Konvertierung behält es`,
    backportLibs: (exclusive, n) =>
      `${exclusive ? "fakelib2/, allein eingebunden" : "fakelib/"}, ${num(n)} ${plural(n, { one: "Backport-Bibliothek", other: "Backport-Bibliotheken" })}`,
    findings: "Befunde der Vorabprüfung",
    dlcTitle: (n) => `Eingebettete DLC (${num(n)})`,
    dlcLabel: "Eingebettete DLC",
    dlcAll: (note) => `Alle DLC: ${note}.`,
    dlcIn: (folder, size) => `in ${folder}/, ${size}`,
    dlcNoData: "freigeschaltet durch den DLC-Emulator (dlc_emu.ini): nur Berechtigung, keine eigenen Daten",
    dlcStatus: (status) => `freigeschaltet durch den DLC-Emulator (dlc_emu.ini), Status ${status || "nicht gesetzt"}`,
    dlcMerged: "in die Ordner des Spiels eingefügt",
    backportFiles: "Backport-Dateien",
    emulatorsIn: "Emulatoren in fakelib",
    qr: (url) => `QR-Code: ${url}`,
    needsPassword: "Dieses Archiv braucht ein Passwort.",
    enterPassword: "Passwort eingeben",
    inArchive: (ext, size) => `in einem ${ext}-Archiv · ${size}`,
    archive: "Archiv",
    parts: (n) => `${num(n)} ${plural(n, { one: "Teil", other: "Teile" })}`,
    archiveRoot: (root) => `Spiel in ${root}`,
    archiveAtRoot: "Spiel im Stamm des Archivs",
    solid: "solid",
    encrypted: "passwortgeschützt",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "Diesen Ordner löschen?" : "Dieses Image löschen?"),
    text: (folder) =>
      folder
        ? "Der Ordner und sein gesamter Inhalt werden endgültig gelöscht: kein Papierkorb, kein Rückgängig. Bricht das Löschen mittendrin ab, kann ein Teil bereits weg sein."
        : "Die Image-Datei wird endgültig gelöscht: kein Papierkorb, kein Rückgängig. Bricht das Löschen mittendrin ab, kann ein Teil bereits weg sein.",
    no: "Nein",
    yes: "Ja, löschen",
    archiveTitle: "Dieses Archiv löschen?",
    archiveText:
      "Die Archivdatei wird endgültig gelöscht: kein Papierkorb, kein Rückgängig. Die übrigen Teile eines mehrteiligen Archivs bleiben, wie sie sind.",
  },
  // password.tsx
  password: {
    title: "Dieses Archiv braucht ein Passwort",
    text: "Geben Sie sein Passwort ein, um es zu öffnen.",
    wrong: "Das Passwort ist nicht korrekt.",
    label: "Passwort",
    cancel: "Abbrechen",
    unlock: "Entsperren",
    checking: "Wird geprüft…",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "Der Payload-Loader auf Port 9021 ist nicht erreichbar. Starten Sie zuerst Ihren Autoloader (Jailbreak) und öffnen Sie Dump Forge dann erneut.",
      missing:
        "Keine gespeicherte Kopie von Dump Forge gefunden (weder in /data/dumpforge/ noch auf einem USB-Laufwerk). Laden Sie es einmal von einem PC; es speichert dann eine Kopie für dieses Symbol.",
      timeout:
        "Dump Forge wurde gestartet, antwortet aber nach 30 Sekunden noch nicht. Vielleicht startet es noch (eine Konsole, die aus dem Ruhemodus erwacht, ist langsam).",
      stopping: "Das vorherige Dump Forge wird noch beendet (es bricht seine Aufträge ab und räumt auf). Warten Sie kurz und versuchen Sie es dann erneut.",
    },
    loadFailed: (e) => `Die Seite konnte nicht geladen werden: ${e}`,
    noAnswer: "Dump Forge hat nicht geantwortet",
    cantStart: "Dump Forge kann nicht gestartet werden",
    retry: "Erneut versuchen",
    starting: "Dump Forge wird gestartet…",
    client: {
      unreachable:
        "Dump Forge ist nicht erreichbar. Prüfen Sie, ob die Payload noch läuft und dieses Gerät im selben Netzwerk ist.",
      noPlatform: "Es lässt sich nicht feststellen, ob Dump Forge auf einer PS5 läuft. Laden Sie die Seite neu, um es erneut zu versuchen.",
      quitUnreachable: "Dump Forge ist nicht erreichbar und kann nicht beendet werden.",
      stillAnswering: "Dump Forge antwortet nach 2 Minuten immer noch; prüfen Sie die PS5.",
      gone: "wird von Dump Forge nicht mehr gemeldet (aus dem Auftragsverlauf entfernt, bevor sein Ergebnis sichtbar war)",
    },
  },
  // target.tsx
  target: {
    format: {
      folder: { text: "Ein Image oder Paket zurück in einen normalen Spielordner entpacken." },
      exfat: {
        text: "Für Spiele, die als ffpkg Probleme machen und nur wie Inhalte von einem externen Laufwerk laufen. Lässt sich auch auf Mac oder PC öffnen.",
      },
      ffpkg: {
        tone: "good",
        text: "Empfohlen. Das eigene Dateisystem der PS5 (UFS2). Beschreibbares Einbinden möglich.",
      },
      ffpfs: {
        tone: "warn",
        text: "Experimentell. Das PFS-Dateisystem der PS5, unkomprimiert. Dateinamen müssen reines ASCII sein.",
      },
      ffpfsc: {
        tone: "warn",
        text: "Experimentell. Am kleinsten: ein komprimierter Container um ein Image, immer schreibgeschützt eingebunden.",
      },
      pkg: {
        text: "Wird wie ein Store-Spiel installiert und läuft mit nativem Tempo. Benötigt kstuff, fpkg-enable und ppr-patch.",
      },
      lz4: {
        tone: "warn",
        text: "Experimentell. Ein Spielordner, dessen Assets in LZ4-Volumes gepackt sind, die der mitgelieferte AMPR-Emulator (ampr_emu) während des Spiels entpackt. Nur für Spiele mit AMPR.",
      },
    },
    innerFfpkg: { tone: "good", text: "Empfohlen. Das eigene Dateisystem der PS5 (UFS2)." },
    level0: { text: "Keine Kompression: Jeder Block wird unverändert gespeichert. Die Datei ist so groß wie das Image." },
    levelDefault: {
      tone: "good",
      text: "Empfohlen: die Standardstufe von zlib. Mit Stufe 1 dauert das Erstellen etwa ein Drittel so lange; die Datei wird dabei etwa 3 % größer. Mit Stufe 9 wird sie kaum kleiner.",
    },
    levelLow: {
      text: "Das Erstellen dauert weniger lange als mit Stufe 6 (mit Stufe 1 etwa ein Drittel so lange); die Datei wird größer (mit Stufe 1 etwa 3 % größer).",
    },
    levelHigh: {
      text: "Kaum kleiner als 6 (Stufe 9 um 0,04 %), aber die Kompression dauert länger (Stufe 9 etwa 25 %), und alle Kerne sind durchgehend ausgelastet.",
    },
    kraken: {
      fast: { label: "Schnell", note: { tone: "good", text: "Empfohlen. Die kürzeste Erstellungszeit." } },
      balanced: {
        label: "Ausgewogen",
        note: {
          text: "Etwa 2,6 % kleiner als „Schnell“, aber die Kompression dauert etwa 6-mal so lange, und alle Kerne sind die ganze Zeit ausgelastet.",
        },
      },
      smallest: {
        label: "Am kleinsten",
        note: {
          text: "Etwa 2,7 % kleiner als „Schnell“, aber die Kompression dauert etwa 9-mal so lange, und alle Kerne sind die ganze Zeit ausgelastet.",
        },
      },
      stored: {
        label: "Unkomprimiert",
        note: {
          text: "Keine Kompression: Das Paket ist etwa so groß wie das Spiel. Probieren Sie es, wenn sich ein komprimiertes Paket installieren, aber nicht starten lässt (gemeldet bei Firmware 8.60 und 10.40).",
        },
      },
    },
    pkgFirmware: "Funktioniert nur mit Firmware 11.60 oder niedriger.",
    imageInside: "Inneres Image",
    imageInsideLabel: "Image im ffpfsc",
    compressionLevel: "Kompressionsstufe",
    compression: "Kompression",
    fullVerify: "Vollständige Prüfung",
    fullVerifyHint: "Liest jedes Byte erneut, um die Ausgabe zu prüfen. Dauert länger.",
    separator: "Ein Name darf kein Ordnertrennzeichen enthalten; den Ordner ändern Sie mit „Ändern“.",
    badName: "Ein Name darf nicht leer sein, nicht mit einem Punkt beginnen, nicht auf Punkt oder Leerzeichen enden und keine Steuerzeichen enthalten.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Zu lang: ${bytes} Byte, höchstens ${max} (UTF-8).`
        : `Zu lang: ${bytes} Byte vor der Endung, höchstens ${max} (UTF-8).`,
    pkgExt: (ext) => `Endet auf .${ext}, aber fpkg wird als .pkg gespeichert: Die Endung bestimmt den Treiber.`,
    otherExt: (ext, target) =>
      `Endet auf .${ext}, aber das Ziel ist ${target}: Die Endung bestimmt den Treiber.`,
    extractInto: "In diesen Ordner entpacken",
    saveInto: "In diesem Ordner speichern",
    output: "Ausgabe",
    change: "Ändern",
    naming: "Name wird ermittelt…",
    chooseSourceFirst: "Zuerst eine Quelle wählen",
    fileName: "Dateiname",
    savedAs: (name, dir) => `wird als ${name} in ${dir} gespeichert`,
    inDir: (dir) => `in ${dir}`,
    generate: "Namen aus dem Inhalt erzeugen",
    generatePattern: "[Spielname]-[Titel-ID]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} ist verfügbar`,
    update: "Aktualisieren",
    later: "Später",
    updating: "Wird aktualisiert…",
    updatingNote: "Die neue Version wird heruntergeladen, die App startet von selbst neu.",
    downloading: "Der Download läuft im Browser: Entpacken Sie das Archiv und ersetzen Sie diese Kopie damit.",
    failed: (v) => `Aktualisierung auf v${v} fehlgeschlagen`,
    timeout: (v) => `v${v} ist nicht innerhalb einer Minute gestartet. Wenn sie heruntergeladen wurde, beenden Sie Dump Forge und öffnen Sie es erneut, um das Update abzuschließen.`,
    retry: "Erneut versuchen",
    close: "Schließen",
  },
  // api.ts: prettyDuration's units
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default de;
