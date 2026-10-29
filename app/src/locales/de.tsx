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
    screens: "Bereiche",
    convert: "Konvertieren",
    active: (n) => `, ${num(n)} aktiv`,
    edit: "Bearbeiten",
    pack: "Packen",
    storage: "Speicher",
    connect: "Verbinden",
    disconnected: "Nicht verbunden",
    connected: (n: number) => plural(n, { one: `Verbunden (${num(n)} Gerät)`, other: `Verbunden (${num(n)} Geräte)` }),
    about: "Info",
    prevTab: "Vorheriger Tab (L2)",
    nextTab: "Nächster Tab (R2)",
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
    quitting: "PS5 Dump Forge wird beendet…",
    couldntStop: "Dump Forge konnte nicht beendet werden",
    close: "Schließen",
    tryAgain: "Erneut versuchen",
    stopped: "Dump Forge wurde beendet",
    startAgain: (ps5) => (ps5 ? "Um fortzufahren, öffnen Sie die Kachel Dump Forge auf der PS5 erneut." : "Um fortzufahren, führen Sie denselben Befehl ps5-dump-forge serve erneut aus."),
    confirmStop: (web: boolean) => (web ? "Dump Forge beenden?" : "PS5 Dump Forge beenden?"),
    stopNote: (web, ps5) => (!web ? "Es läuft kein Auftrag." : ps5 ? "Zum erneuten Starten öffnen Sie die Kachel Dump Forge auf der PS5." : "Zum erneuten Starten führen Sie denselben Befehl ps5-dump-forge serve noch einmal aus."),
    peerBusy: "Ein gekoppeltes Gerät liest oder schreibt Dateien auf diesem Computer. Beim Beenden bricht seine Übertragung ab.",
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
      "Die Assets dieses Spiels sind LZ4-gepackt und sein LZ4-Manifest enthält den Backport: Entpacken Sie es zuerst im Tab „Packen“ und entfernen Sie dann hier den Backport.",
    zstdBackport:
      "Die Assets dieses Spiels sind zstd-gepackt und sein zstd-Index enthält den Backport: Entpacken Sie es zuerst im Tab „Packen“ und entfernen Sie dann hier den Backport.",
    unreadable: "Erstellen nicht möglich: Diese Quelle konnte nicht gelesen werden (siehe „Quelle“).",
    patchBlocked: "Erstellen nicht möglich: Das ist ein Patch-Paket, das nur enthält, was das Update geändert hat. Öffne es unter „Bearbeiten“, um seine Dateien zu extrahieren.",
    openInEdit: "Unter „Bearbeiten“ öffnen",
    noContentId: "Erstellen als fpkg nicht möglich: Die param.json dieses Spiels hat keine Content-ID, die ein Paket braucht.",
    blocked: (first, more) =>
      `Erstellen nicht möglich: ${first}${more > 0 ? ` (und ${num(more)} weitere, siehe „Quelle“)` : ""}.`,
    amprZstdRuntime: "Diese Konvertierung installiert ampr-emu-ng unter fakelib/libSceAmpr.sprx, das diese Pakete brauchen.",
    amprNgReplaced: (version) => `Ersetzt das ampr-emu-ng dieses Spiels durch ampr_emu ${version}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Dieses Spiel nutzt AMPR, hat aber kein ampr_emu: Die Konvertierung installiert das mitgelieferte ampr_emu ${version}.`;
      if (a === "trace") return `Die Konvertierung ersetzt das ampr_emu dieses Spiels durch das mitgelieferte ampr_emu ${version}.`;
      if (a === "replace")
        return own
          ? `Die Konvertierung ersetzt das ampr_emu ${own} dieses Spiels durch das mitgelieferte ampr_emu ${version}.`
          : `Die Konvertierung ersetzt das ampr_emu dieses Spiels, das keine Version nennt, durch das mitgelieferte ampr_emu ${version}.`;
      return own
        ? `Das ampr_emu ${own} dieses Spiels ist so neu wie das mitgelieferte ${version} oder neuer: Die Konvertierung behält es.`
        : `Dieses Spiel hat bereits das mitgelieferte ampr_emu ${version}: Die Konvertierung behält es.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index listet nicht die Dateien dieses Spiels" : state === "damaged" ? "ampr_emu.index ist nicht lesbar" : "Dieses Spiel hat keine ampr_emu.index"}: Die Konvertierung schreibt eine neue.`,
    amprPkgIndex: "Das Paket erhält eine neue ampr_emu.index aus seinen Dateien.",
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
      zstd: { tagline: "Für Spiele mit AMPR, mit ampr-emu-ng", cells: ["Bis zu 80 % schnelleres Laden", "Bis zu halb so groß wie das Spiel"] },
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
      ["Übertragen", "von Dateien und Konvertieren von Spielen zwischen Ihrer PS5, Computern und einem NAS über SMB, FTP oder NFS."],
      ["Bearbeiten", "der Dateien eines Spiels wie in einem Ordner, dazu Cover, Firmware, Backport, DLC und Probleme mit Dateinamen vor dem Erstellen."],
      ["Gestreamt", "von jeder Quelle zu jedem Ziel: ohne temporäre Kopie, auch für das Image in einem ffpfsc oder ein Spiel in einem Archiv."],
      ["Archive", "ein .zip, .7z oder .rar mit einem Spiel wird wie ein Ordner durchsucht, konvertiert oder gepackt."],
      ["USB und SD-Karten", "geschrieben auf Sticks und Karten mit exFAT oder FAT32."],
      ["Packen", "Asset-Pakete mit zstd oder LZ4 (experimentell) und intelligenter Reparatur."],
      ["Speicher", "eine Bibliothek Ihrer Spiele und ein Datei-Explorer zum Umbenennen, Verschieben und Löschen."],
      ["Sicher", "Ihr Spiel wird nie verändert, außer Sie wünschen es; Müll-Dateien werden übersprungen, ungültige Dateinamen vorab gemeldet."],
    ],
    about: "Info",
    compare: "Formate vergleichen",
    compareLabel: "Formatvergleich",
    format: "Format",
    credits: "Danksagungen",
    upstream: (app) =>
      `Angaben der Autoren: was die Autoren von ampr_emu und Lazy_AMPR berichten, nicht von ${app} gemessen; die Werte schwanken je nach Spiel. ampr_emu 0.4.2.1 ist ein Test-Build der Autoren; bekanntes Problem: Manche Spiele stürzen beim Speichern ab.`,
    bundled: "LZ4-gepackte Ordner laufen mit ampr_emu 0.4.2.1 von drakmor (GPL-3.0), unverändert mitgeliefert. Quellcode:",
    lazy: "Das LZ4-Packen baut auf Lazy_AMPR von Nazky auf:",
    ng: "zstd- und LZ4-Pakete laufen auf ampr-emu-ng. Quelle:",
    logo: "Das Logo stammt von Xxcco:",
    unrar:
      ".rar-Archive werden mit der mitgelieferten UnRAR-Bibliothek von Alexander Roshal gelesen (nur Extrahieren, unter der UnRAR-Lizenz).",
    language: "Sprache",
    system: "System",
    notifications: "Nach Abschluss eines Auftrags benachrichtigen",
    notificationsHint: "Wird auf der Konsole für Konvertierungs-, Pack- und Übertragungsaufträge angezeigt.",
    notificationsFailed: "Benachrichtigungen konnten nicht geändert werden.",
  },
  // Edit.tsx
  edit: {
    details: "Details",
    deleting: "Quelle wird gelöscht",
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
      "Öffnen Sie einen **Spielordner** oder ein **Image** und ändern Sie es wie einen Ordner auf einem USB-Stick: **Dateien hinzufügen und ersetzen** (hineinziehen oder „Hinzufügen“ verwenden), **umbenennen und löschen**.",
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
    actions: "Aktionen",
    items: (n) => `${num(n)} ${plural(n, { one: "Element", other: "Elemente" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "Ordner", other: "Ordner" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "Treffer", other: "Treffer" })}`,
    emptyFolder: "Dieser Ordner ist leer.",
    open: (name) => `${name} öffnen`,
    archiveNote: "Archive können durchsucht, aber nicht bearbeitet werden. Um es zu ändern, konvertieren Sie es zuerst in einen Ordner oder ein Image.",
    nasNote: "Spiele auf einem NAS lassen sich durchsuchen und extrahieren, aber nicht bearbeiten. Konvertiere oder extrahiere das Spiel zuerst auf dieses Gerät, um es zu ändern.",
    patchNote: "Das ist ein Patch-Paket: Es enthält nur, was das Update geändert hat. Aufgeführt sind die Dateien, die es vollständig enthält; sie lassen sich durchsuchen und extrahieren, aber nicht bearbeiten.",
    peerNote: "Spiele auf einem anderen Forge lassen sich durchsuchen und extrahieren, aber nicht bearbeiten. Konvertiere oder extrahiere das Spiel zuerst auf dieses Gerät, um es zu ändern.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "Dateien hinzufügen",
    addFolder: "Ordner hinzufügen",
    pickAddFiles: "Dateien zum Hinzufügen wählen",
    pickAddFolder: "Ordner zum Hinzufügen wählen",
    peerRoot: "Ein ganzes Gerät kann nicht hinzugefügt werden: Wählen Sie einen Ordner darin.",
    renameItem: (name) => `${name} umbenennen`,
    removeItem: (name) => `${name} löschen`,
    renameTip: "Umbenennen (F2)",
    removeTip: "Löschen (Entf-Taste)",
    removeTipMac: "Löschen (fn+Rückschritttaste)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extractItem: (name: string) => `${name} extrahieren`,
    extractTip: "In einen gewählten Ordner kopieren",
    extractFolder: "Ordner extrahieren",
    extractFolderTip: "Den angezeigten Ordner in einen gewählten Ordner kopieren",
    extractGameTip: "Das ganze Spiel in einen gewählten Ordner kopieren",
    extractNotOnDisk: "Dieser Ordner existiert nur in Ihren ausstehenden Änderungen: Wenden Sie sie an, um ihn zu extrahieren.",
    pickExtract: "Zielordner zum Extrahieren wählen",
    extractPending: "Extrahiert die Dateien im jetzigen Zustand, ohne Ihre ausstehenden Änderungen.",
    nameEmpty: "Geben Sie einen Namen ein.",
    nameBad: "Ein Name darf weder / noch \\ enthalten und nicht . oder .. sein",
    nameTaken: (name) => `${name} gibt es hier schon.`,
    markNew: "Neu",
    markReplaced: "Ersetzt",
    markRenamed: "Umbenannt",
    wasAt: (path) => `Vorher ${path}`,
    sizeLater: "Mehr, als aufgelistet werden konnte: Der Rest wird gelesen, wenn die Änderungen angewendet werden",
    dropInto: (folder) => `Ablegen, um zu ${folder} hinzuzufügen`,
    droppedGone: (path) => `${path}: Die hineingezogene Datei ist nicht mehr da`,
    pending: (n) =>
      plural(n, { one: `${num(n)} Änderung noch nicht angewendet`, other: `${num(n)} Änderungen noch nicht angewendet` }),
    undo: "Rückgängig",
    undoTip: "Letzte Aktion mit allen ihren Änderungen rückgängig machen",
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
    nothingToChange: "Nichts zu ändern: Das Spiel bleibt, wie es ist.",
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
    uploadingFile: (i: number, n: number) => `Datei ${num(i)} von ${num(n)} wird hochgeladen`,
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
      install: "Installation",
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
      install: "Installation",
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
    packTag: {
      lz4Pack: (level: string) => `LZ4 packen · ${level}`,
      lz4Unpack: "LZ4 entpacken",
      zstdPack: (level: string) => `zstd packen · ${level}`,
      level: (n: number) => `Stufe ${num(n)}`,
      zstdUnpack: "zstd entpacken",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "Bearbeiten",
    editRebuild: "Bearbeiten · neu erstellt",
    transfer: "Übertragung",
    extract: "Extrahieren",
    job: (id) => `Auftrag ${id}`,
    done: "Fertig",
    cancelled: "Abgebrochen",
    failed: "Fehlgeschlagen",
    cancelling: "Wird abgebrochen…",
    queued: "Wartet",
    dismiss: "Ausblenden",
    retry: "Erneut versuchen",
    retrying: "Wird gestartet",
    stopJob: "Stoppen?",
    cancel: "Abbrechen",
    stopHint: "Drücken Sie innerhalb von 4 Sekunden erneut „Stoppen?“, um den Auftrag zu stoppen.",
    remoteLabel: (name, device) => `${name}, ein Auftrag auf ${device}`,
    remoteTitle: (device) => `Läuft auf ${device} und verwendet Dateien dieses Geräts: Hier können Sie ihn nur abbrechen`,
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
    cantTransfer: "Übertragung nicht möglich:",
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
    library: "Bibliothek",
    storageTip: "Freier Speicher im internen Speicher der Konsole (/data)",
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
    deleting: (n) => `${n} wird gelöscht`,
    lz4Tip: "LZ4-Asset-Pakete",
    zstdTip: "zstd-Asset-Pakete",
    showAs: "Anzeigen als",
    kinds: "Art anzeigen",
    all: "Alle",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n, path) => (
      <>
        <b>{num(n)}</b> {plural(n, { one: "Spiel in", other: "Spiele in" })} {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "Speicheransicht",
    library: "Bibliothek",
    files: "Datei-Explorer",
    title: "Datei-Explorer",
    pickTitle: "Ordner zum Durchsuchen wählen",
    pickPrompt: "Wählen Sie einen Ordner, um seine Dateien anzuzeigen",
    choose: "Ordner wählen",
    roots: "Laufwerke",
    location: "Ort",
    folders: "Ordner",
    up: "Nach oben",
    upTip: "Einen Ordner nach oben",
    refresh: "Aktualisieren",
    filter: "Nach Name filtern",
    filterLabel: "Diesen Ordner nach Name filtern",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "Ordner", other: "Ordner" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "Treffer", other: "Treffer" })}`,
    firstOf: (max, n) => `erste ${num(max)} von ${num(n)}`,
    truncated: "Dieser Ordner enthält mehr als 10.000 Einträge: Nur die ersten 10.000 werden angezeigt.",
    name: "Name",
    size: "Größe",
    emptyFolder: "Dieser Ordner ist leer.",
    noMatch: (f) => `Hier passt nichts zu „${f}“.`,
    noRoots: "Keine Laufwerke zum Durchsuchen verfügbar.",
    cantList: "Dieser Ordner kann nicht geöffnet werden:",
    link: "Link",
    linkTip: "Ein Link: Er lässt sich öffnen, aber hier nicht umbenennen, verschieben oder löschen",
    mounted: "Eingehängt",
    mountedTip: "Von ShadowMount+ eingehängt: Beenden Sie das Spiel und schalten Sie ShadowMount+ aus, um es zu löschen, umzubenennen oder zu verschieben",
    mountedNote: "Mit „Eingehängt“ markierte Images verwendet ShadowMount+: Beenden Sie das Spiel und schalten Sie ShadowMount+ aus, um sie zu löschen, umzubenennen oder zu verschieben.",
    open: (n) => `${n} öffnen`,
    more: (n) => `Weitere Aktionen für ${n}`,
    rename: (n) => `${n} umbenennen`,
    renameTip: "Umbenennen (F2)",
    move: (n) => `${n} verschieben`,
    moveTip: "In einen anderen Ordner verschieben",
    moveTitle: (n) => `${n} verschieben nach`,
    remove: (n) => `${n} löschen`,
    removeTip: "Löschen (Entf-Taste)",
    removeTipMac: "Löschen (fn+Rückschritttaste)",
    cantDelete: "Löschen nicht möglich:",
    cantRename: "Umbenennen nicht möglich:",
    cantMove: "Verschieben nicht möglich:",
    sameFolder: "Es liegt bereits in diesem Ordner.",
    deleted: (path) => `Gelöscht: ${path}`,
    renamed: (path) => `Umbenannt in ${path}`,
    moved: (path) => `Verschoben nach ${path}`,
    devices: "Geräte",
    thisDevice: "Dieses Gerät",
    otherDevice: "„Verschieben“ bleibt auf einem Gerät: Ziehen Sie es auf den Tab eines anderen Geräts, um es dorthin zu kopieren.",
    dropCopy: (folder) => `Loslassen, um nach ${folder} zu kopieren`,
    dropMove: (folder) => `Loslassen, um nach ${folder} zu verschieben`,
    cantCopy: "Kopieren nicht möglich:",
    copyNoFolder: "Öffnen Sie zuerst einen Ordner auf jenem Gerät.",
    copyOne: (name, to) => `${name} wird nach ${to} kopiert. Die Übertragung steht unter „Verbinden“.`,
    copyMany: (n, to) =>
      plural(n, {
        one: `${num(n)} Element wird nach ${to} kopiert. Die Übertragung steht unter „Verbinden“.`,
        other: `${num(n)} Elemente werden nach ${to} kopiert. Die Übertragung steht unter „Verbinden“.`,
      }),
    openConnect: "„Verbinden“ öffnen",
  },
  // Pack.tsx
  pack: {
    title: "Packen",
    help: "So funktioniert Packen",
    engine: "Art der Pakete",
    engines: { zstd: "zstd", lz4: "LZ4" },
    actions: { pack: "Packen", unpack: "Entpacken" },
    info: (e, packed) =>
      e === "zstd"
        ? packed
          ? "Schreibt die zstd-Pakete als normale Dateien zurück."
          : "Packt die Assets des Spiels in zstd-Volumes."
        : packed
          ? "Experimentell. Schreibt die LZ4-Pakete als normale Dateien zurück."
          : "Experimentell: Manche Spiele starten eventuell nicht. Packt die Assets des Spiels in LZ4-Volumes.",
    chooseGame: "Wählen Sie zuerst ein Spiel.",
    noAmprGame: (e) => `Dieses Spiel nutzt kein AMPR, daher sind ${e === "zstd" ? "zstd" : "LZ4"}-Pakete nicht nutzbar.`,
    patch: "Ein Patch-Paket lässt sich nicht packen.",
    additionalContent: "Zusatzinhalte (ohne eboot.bin) lassen sich nicht packen.",
    unreadable: "Start nicht möglich: Die Quelle konnte nicht gelesen werden (siehe „Quelle“).",
    blocked: (a, first, more) =>
      `${a === "pack" ? "Packen" : "Entpacken"} nicht möglich: ${first}${more > 0 ? ` (und ${num(more)} weitere, siehe „Quelle“)` : ""}.`,
    target: "Ziel",
    packFolder: (e) => `Empfohlen. Am schnellsten, geringste Latenz: ein neuer Spielordner, seine Assets in ${e === "zstd" ? "zstd" : "LZ4"}-Paketen.`,
    levels: { fast: "Schnell", medium: "Mittel", max: "Maximal" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Empfohlen. zstd 5: schnell, gute Größe." },
        medium: { text: "zstd 9: kleiner, langsamer." },
        max: { text: "zstd 19: am kleinsten; viel langsamer, mehr Speicher." },
      },
      lz4: {
        fast: { tone: "good", text: "Empfohlen. LZ4-Standard: am schnellsten." },
        medium: { text: "LZ4 HC 9: kleiner, langsamer." },
        max: { text: "LZ4 HC 12: am kleinsten, am langsamsten." },
      },
    },
    runtime: "Runtime",
    runtimeNotes: {
      ng: { tone: "good", text: "Empfohlen. Liest zstd- und LZ4-Pakete." },
      ampr_emu: { text: "Das Original-ampr_emu." },
    },
    ngNote: "Braucht ampr-emu-ng: „Packen“ installiert es in fakelib/. Schalten Sie das fakelib-Update von ShadowMount+ aus, sonst ersetzt es die Runtime.",
    installsRuntime: (version) => `Hier ist kein ampr_emu: „Packen“ installiert ampr_emu ${version}.`,
    freshIndex: (state) =>
      `${state === "stale" ? "ampr_emu.index ist veraltet" : state === "damaged" ? "ampr_emu.index ist beschädigt" : "Keine ampr_emu.index"}: „Packen“ schreibt eine neue.`,
    direct: "Packt direkt in das Image; das Spiel wird zweimal gelesen.",
    unpackFolder: "Ein neuer Spielordner mit normalen Dateien.",
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
    emptyTitle: "Wählen Sie ein Spiel mit AMPR",
    experimental: "Experimentell: Bewahren Sie eine Kopie des Originalspiels auf.",
    source: "Quelle",
  },
  // PackHelp.tsx
  packHelp: {
    title: "So funktioniert Packen",
    close: "Schließen",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>Packt die Assets des Spiels in zstd-Volumes, in einen Ordner oder ein Image. Nur für Spiele mit AMPR.</li>
          <li><b>Schnell</b> (zstd 5) ist flott; <b>Mittel</b> (9) und <b>Maximal</b> (19) sind kleiner, aber langsamer.</li>
          <li>Ein gepacktes Spiel zeigt <b>Entpacken</b>: Die normalen Dateien kommen zurück.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Experimentell</b>: Manche Spiele starten eventuell nicht. Bewahren Sie eine Kopie auf.</li>
          <li>Packt in LZ4-Volumes, in einen Ordner oder ein Image: <b>Schnell</b> (LZ4), <b>Mittel</b> (HC 9), <b>Maximal</b> (HC 12).</li>
          <li>Runtime: <b>ampr-emu-ng</b> (Standard) oder das Original-<b>ampr_emu</b>.</li>
          <li>Ein gepacktes Spiel zeigt <b>Entpacken</b>.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>Liest zstd- und LZ4-Pakete; „Packen“ installiert es in fakelib/.</li>
          <li>Schalten Sie das fakelib-Update von ShadowMount+ aus, sonst ersetzt es die Runtime und das Spiel kann seine Pakete nicht lesen.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "Geräte",
    thisDevice: "Dieses Gerät",
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
    chooseFiles: (n) => (n === 0 ? "Dateien wählen" : `${num(n)} ${plural(n, FILES)} wählen`),
    nothing: "Hier gibt es keine Ordner oder Dateien.",
    cantList: "Dieser Ordner kann nicht geöffnet werden:",
    cancel: "Abbrechen",
    chooseFolder: "Diesen Ordner wählen",
  },
  // common.tsx
  common: {
    folder: "Ordner",
    pickButton: "Auswählen",
    pickAny: "Spielordner, Image oder Archiv",
    source: "Quelle",
    emptyTitle: "Wählen Sie einen Spielordner, ein Image oder ein Archiv",
    accepted: "Unterstützte Quellen",
    unknownTitle: "Unbekannter Titel",
    patch: "Patch",
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
      forge_trace: "alte LZ4-Trace-Runtime",
      zstd: "ampr-emu-ng",
      other: "andere",
      none: "keine",
    },
    packed: "Gepackt",
    zstdKept: "zstd-Asset-Pakete: Eine Konvertierung behält sie",
    damaged: "Beschädigte Pakete",
    plain: "Ungepackt",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} in ${num(volumes)} ${plural(volumes, { one: "Volume", other: "Volumes" })}` +
      (percent === null ? "" : `, ${num(percent)} % der Größe`),
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
        : "Die Image-Datei wird endgültig gelöscht: kein Papierkorb, kein Rückgängig.",
    no: "Nein",
    yes: "Ja, löschen",
    archiveTitle: "Dieses Archiv löschen?",
    archiveText:
      "Die Archivdatei wird endgültig gelöscht: kein Papierkorb, kein Rückgängig. Die übrigen Teile eines mehrteiligen Archivs bleiben, wie sie sind.",
    fileTitle: "Diese Datei löschen?",
    fileText: "Die Datei wird endgültig gelöscht: kein Papierkorb, kein Rückgängig.",
  },
  // replace.tsx
  replace: {
    title: (n) =>
      plural(n, { one: `${num(n)} Element gibt es dort schon`, other: `${num(n)} Elemente gibt es dort schon` }),
    inFolder: (folder) => <>In {folder}:</>,
    more: (n) => `und ${num(n)} weitere`,
    text: "„Ersetzen“ setzt die neuen an ihre Stelle: Die alten Dateien sind endgültig weg, ohne Papierkorb und ohne Rückgängig. Ein Ordner auf einem gleichnamigen Ordner wird zusammengeführt: Dateien mit gleichen Namen werden ersetzt, alles andere dort bleibt.",
    cancel: "Abbrechen",
    replace: "Ersetzen",
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
      uploadIncomplete: "Der Upload wurde nicht abgeschlossen.",
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
        text: "Das eigene Dateisystem der PS5 (UFS2). Beschreibbares Einbinden möglich.",
      },
      ffpfs: {
        text: "Das PFS-Dateisystem der PS5, unkomprimiert. Dateinamen müssen reines ASCII sein.",
      },
      ffpfsc: {
        text: "Am kleinsten: ein komprimierter Container um ein Image, immer schreibgeschützt eingebunden.",
      },
      pkg: { tone: "good", text: "Empfohlen. Läuft wie ein natives Spiel, aber nur, wenn Ihre Firmware es zulässt." },
      lz4: {
        tone: "warn",
        text: "Experimentell. Ein Spielordner, dessen Assets in LZ4-Volumes gepackt sind, die der mitgelieferte AMPR-Emulator (ampr_emu) während des Spiels entpackt. Nur für Spiele mit AMPR.",
      },
    },
    innerFfpkg: { text: "Das eigene Dateisystem der PS5 (UFS2)." },
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
    updatingNote: "Die neue Version wird heruntergeladen. Diese Seite lädt sie danach von selbst neu.",
    downloading: "Der Download läuft im Browser: Entpacken Sie das Archiv und ersetzen Sie diese Kopie damit.",
    failed: (v) => `Aktualisierung auf v${v} fehlgeschlagen`,
    timeout: (v) => `v${v} ist nicht innerhalb einer Minute gestartet. Wenn sie heruntergeladen wurde, beenden Sie Dump Forge und öffnen Sie es erneut, um das Update abzuschließen.`,
    retry: "Erneut versuchen",
    close: "Schließen",
  },
  // api.ts: prettyDuration's units
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "Dieses Gerät",
    howItWorks: "So verbinden Sie",
    ipAddress: "IP-Adresse",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? plural(n, { one: `gesehen: vor ${num(n)} Minute`, other: `gesehen: vor ${num(n)} Minuten` })
        : unit === "h"
          ? plural(n, { one: `gesehen: vor ${num(n)} Stunde`, other: `gesehen: vor ${num(n)} Stunden` })
          : unit === "d"
            ? plural(n, { one: `gesehen: vor ${num(n)} Tag`, other: `gesehen: vor ${num(n)} Tagen` })
            : "gesehen: gerade eben",
    listenError: (e) => `Andere Geräte erreichen dieses nicht: ${e}`,
    unavailable: (e) => `Verbinden ist nicht verfügbar: ${e}`,
    loading: "Wird geladen",
    discover: "Automatische Suche",
    stop: "Stoppen",
    fileTransfer: "Dateiübertragung",
    devices: "Geräte",
    show: "Anzeigen",
    all: "Alle",
    onlineOf: (n, total) => `${num(n)} von ${num(total)} online`,
    sendFilesTo: (name) => `Dateien an ${name} senden`,
    details: (name) => `Details zu ${name}`,
    userIs: (user) => `Benutzer ${user}`,
    nearby: "In der Nähe",
    nearbyNone: "Keine neuen Geräte gefunden",
    addDevice: "Gerät hinzufügen",
    addressExample: "192.168.1.20 oder nas.local",
    addByProtocol: "Oder nach Protokoll hinzufügen:",
    savedEmpty: "Noch keine gespeicherten Geräte",
    savedEmptyHint:
      "Die automatische Suche findet Forge und Netzwerkspeicher in diesem Netzwerk. Koppeln Sie ein Gerät und erlauben Sie die Kopplung dort, oder speichern Sie ein NAS.",
    online: "Online",
    offline: "Offline",
    computer: "Computer",
    address: "Adresse",
    keyId: "Schlüssel-ID",
    lastSeen: "Zuletzt gesehen",
    status: "Status",
    never: "Nie",
    unknown: "Unbekannt",
    forget: "Vergessen",
    forgetNote: "Vergessen entfernt die Kopplung nur auf diesem Gerät. Um es wieder zu nutzen, koppeln Sie es erneut.",
    close: "Schließen",
    cancel: "Abbrechen",
    pairTitle: "Gerät koppeln",
    pair: "Koppeln",
    connectTo: "Verbinden",
    probing: (address) => `${address} wird geprüft`,
    pairing: "Wird gekoppelt",
    waitingFor: (address) => `Warten, bis ${address} die Kopplung erlaubt`,
    checkNumber: "Prüfnummer",
    checkHint: "Sie muss mit der Nummer übereinstimmen, die das andere Gerät zeigt.",
    denied: "Das andere Gerät hat die Kopplung abgelehnt.",
    timedOut: "Auf dem anderen Gerät hat niemand die Kopplung rechtzeitig erlaubt.",
    pairBusy: "Auf dem anderen Gerät warten noch andere Kopplungsanfragen. Versuchen Sie es in einer Minute erneut.",
    alreadyPaired: "Dieses Gerät ist dort bereits gespeichert. Vergessen Sie es dort zuerst und koppeln Sie dann erneut.",
    tooManyAttempts: "Zu viele abgelehnte Anfragen. Versuchen Sie es in 10 Minuten erneut.",
    pairPromptTitle: (name) => `Mit ${name} koppeln?`,
    pairPromptBody: (name, platform, ip) =>
      `${name} (${platform}, ${ip}) möchte sich mit diesem Gerät koppeln. Ein gekoppeltes Gerät kann die Dateien dieses Geräts lesen und neue hinzufügen.`,
    pairPromptCheck: (name) => `Erlauben Sie es nur, wenn ${name} dieselbe Prüfnummer zeigt.`,
    allow: "Erlauben",
    deny: "Ablehnen",
    notPaired: "Das andere Gerät kennt dieses nicht mehr. Koppeln Sie es erneut.",
    searching: "Dieses Netzwerk wird durchsucht",
    forge: "Forge",
    save: "Speichern",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `${protocol}-NAS hinzufügen`,
    nasEditTitle: (protocol) => `${protocol}-NAS bearbeiten`,
    nasName: "Name",
    nasAddressHint: "IP-Adresse oder Hostname; mit :Port, wenn er nicht 445 ist",
    nasUser: "Benutzername",
    nasUserHint: "Leer lassen, um sich als Gast anzumelden",
    ftpAddressHint: "IP-Adresse oder Hostname; :Port angeben, wenn er nicht 21 ist",
    nfsAddressHint: "IP-Adresse oder Hostname; :Port angeben, wenn er nicht 2049 ist",
    ftpUserHint: "Leer lassen, um sich anonym anzumelden",
    ftpNote: "FTP ist unverschlüsselt, auch die Anmeldung.",
    nfsNote: "NFS hat keine Anmeldung: Forge verbindet sich als root (NFSv4.1, sonst NFSv3), daher muss die Freigabe (Export) des Servers dieses Gerät zulassen. NFS ist unverschlüsselt.",
    nasPassword: "Passwort",
    nasPasswordKeep: "Leer lassen, um das gespeicherte Passwort zu behalten",
    guest: "Gast",
    passwordSaved: "Gespeichert",
    passwordNone: "Keins",
    edit: "Bearbeiten",
    nasForgetNote: "Vergessen entfernt dieses NAS und sein gespeichertes Passwort von diesem Gerät.",
    pickSendFiles: "Zu sendende Dateien",
    pickSendFolder: "Zu sendender Ordner",
    pickSaveTo: "Speicherort",
    thisBrowser: "Dieser Browser",
    browserHint: "Dateien von diesem Telefon oder Computer kommen in einen Ordner auf diesem Gerät, und eine Datei von diesem Gerät speichert dieser Browser.",
    uploadTo: "Zielordner für den Upload",
    uploadPick: "Dateien zum Hochladen wählen",
    uploading: (name, i, n) => `${name} wird hochgeladen (${num(i)} von ${num(n)})`,
    uploaded: "Upload abgeschlossen",
    uploadSkipped: (names) => `Nicht hochgeladen, dort gibt es schon einen Ordner oder Link mit diesem Namen: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `${num(done)} von ${num(n)} Dateien wurde hochgeladen und bleibt auf diesem Gerät.`,
        other: `${num(done)} von ${num(n)} Dateien wurden hochgeladen und bleiben auf diesem Gerät.`,
      }),
    cantStart: "Die Übertragung kann nicht gestartet werden:",
    cantUpload: "Hochladen nicht möglich:",
    pickDownload: "Herunterzuladende Datei",
    transferIntro: "Senden Sie Dateien oder einen Ordner von einem beliebigen Gerät in einen Ordner hier, auf einem gekoppelten Gerät oder auf einem NAS. Die Übertragung läuft als Auftrag in der Auftragsliste.",
    send: "Senden",
    sendWhat: "Was gesendet wird",
    sendTo: "Senden an",
    sendNeedsDevice: "Verbinden Sie zuerst ein anderes Gerät: Koppeln Sie ein Forge oder fügen Sie ein NAS hinzu.",
    nothingChosen: "Noch nichts ausgewählt",
    chooseFiles: "Dateien wählen",
    chooseFolder: "Ordner wählen",
    startTransfer: "Starten",
    uploadHere: "Auf dieses Gerät hochladen",
    downloadHere: "Von diesem Gerät herunterladen",
    consoleNoFiles: "Der Browser der Konsole kann keine Dateien auswählen: Öffnen Sie diese Seite auf einem Telefon oder Computer.",
    wholeRoot: "Ein ganzes Gerät, eine Freigabe oder ein Laufwerk kann nicht gesendet werden: Wählen Sie einen Ordner darin.",
    helpPhone: "Scannen Sie den QR-Code, um diese Seite auf einem Telefon zu öffnen. Dort übertragen „Auf dieses Gerät hochladen“ und „Von diesem Gerät herunterladen“ in „Dateiübertragung“ die Dateien des Telefons.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "Verbinden verknüpft Dump Forge auf Ihrer PS5 und Ihren Computern im selben Netzwerk.",
      "Zum Koppeln starten Sie die „Automatische Suche“ und wählen „Koppeln“ unter „In der Nähe“ oder geben eine Adresse unter „Gerät hinzufügen“ ein und erlauben es dann auf dem anderen Gerät. Erlauben Sie es nur, wenn beide Bildschirme dieselbe Prüfnummer zeigen.",
      "Ein NAS (SMB, FTP, NFS) wird genauso hinzugefügt. Gekoppelte Geräte und NAS sind Spielquellen und Ziele; ihre Spiele werden dort nicht bearbeitet.",
      "„Dateiübertragung“ kopiert Dateien und Ordner zwischen ihnen. Ein Name, der dort schon existiert, wird erst nach Ihrer Bestätigung ersetzt.",
    ],
    helpSecurity: "Nichts, was zwischen Geräten gesendet wird, ist verschlüsselt: Jeder in Ihrem Netzwerk kann es mitlesen. Ein gespeichertes NAS-Passwort liegt unverschlüsselt auf diesem Gerät. Nutzen Sie Verbinden nur im Heimnetz.",
    installTitle: "Dump Forge auf der PS5",
    ps5Address: "PS5-Adresse",
    install: "Auf der PS5 installieren",
    installHint: "Benötigt den ELF-Loader der Konsole, im Netzwerk erreichbar (Port 9021).",
    findingRelease: "Version auf GitHub wird gesucht",
    installing: (v, host) => `Dump Forge ${v} wird auf ${host} installiert`,
    installed: (v, host) => `Dump Forge ${v} startet auf ${host}. Koppeln Sie es unter „Gerät hinzufügen“, sobald es läuft.`,
    noRelease: "Die PS5-Version von Dump Forge wurde auf GitHub nicht gefunden. Prüfen Sie die Internetverbindung dieses Computers und versuchen Sie es erneut.",
    otherPayload: "Jede andere ELF-Payload:",
    sendPayload: "Payload senden",
    pickPayload: "ELF-Payload auswählen",
    sendingPayload: (name, host) => `${name} wird an ${host} gesendet`,
    sentPayload: (name, size, host) => `${name} (${size}) an ${host} gesendet.`,
    helpInstallTitle: "Dump Forge mit einem Klick auf einer PS5 installieren",
    helpInstallText: "Senden Sie die Payload mit der Karte zur Installation von Dump Forge an die Konsole. Sie lädt die passende Version von GitHub (oder die neueste) und startet sie.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "Paketinstallation",
    packages: "Pakete",
    refresh: "Aktualisieren",
    scanning: "Pakete werden gesucht",
    none: "Auf dieser Konsole wurden keine Pakete gefunden.",
    noneHint: "Kopieren Sie eine .pkg-Datei auf den Speicher der Konsole oder auf ein USB-Laufwerk und drücken Sie dann Aktualisieren.",
    truncated: "Liste eventuell unvollständig",
    limits: "Die Suche hat an einer ihrer Grenzen angehalten: Einige Pakete fehlen eventuell.",
    cantScan: "Suche nicht möglich:",
    cantInstall: "Installation nicht möglich:",
    content: { game: "Spiel", patch: "Patch", additional_content: "Zusatzinhalt", unknown: "Unbekannt" },
    install: "Installieren",
    installName: (name) => `${name} installieren`,
    started: (name) => `${name} wird installiert. Der Auftrag steht unten in der Liste.`,
    tag: "Installation",
    title: "Dieses Paket installieren?",
    titleOn: (device) => `Dieses Paket auf ${device} installieren?`,
    file: "Datei",
    game: "Spiel",
    titleId: "Titel-ID",
    version: "Version",
    size: "Größe",
    text: "Die PS5 holt das Paket von Dump Forge und installiert es. Lassen Sie Dump Forge laufen, bis der Auftrag endet.",
    reinstall: "Ist dieses Spiel bereits installiert, löscht die PS5 die installierte Kopie, bevor sie diese installiert.",
    cancel: "Abbrechen",
    confirm: "Installieren",
    installPkg: "pkg installieren",
    installPkgOn: (device) => `Ein Paket auf ${device} installieren`,
    pickTitle: "Paket zum Installieren wählen",
    sent: (device) => `${device} installiert das Paket. Der Auftrag steht unten in der Liste.`,
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default de;
