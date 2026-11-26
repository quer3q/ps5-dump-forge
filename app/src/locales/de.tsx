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
    active: (n) => `, ${num(n)} aktiv`,
    edit: "Bearbeiten",
    storage: "Speicher",
    connect: "Verbinden",
    disconnected: "Nicht verbunden",
    connected: (n: number) => plural(n, { one: `${num(n)} Gerät`, other: `${num(n)} Geräte` }),
    about: "Info",
    offline: "Offline, neuer Versuch…",
    stop: "Beenden",
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
  // build/ (Build's convert settings and checks)
  convert: {
    targetFormat: "Format",
    alreadyFolder: "die Quelle ist bereits ein Ordner",
    patchBlocked: "Erstellen nicht möglich: Das ist ein Patch-Paket, das nur enthält, was das Update geändert hat. Öffne es unter „Bearbeiten“, um seine Dateien zu extrahieren.",
    openInEdit: "Unter „Bearbeiten“ öffnen",
    noContentId: "Erstellen als FPKG nicht möglich: Die param.json dieses Spiels hat keine Content-ID, die ein Paket braucht.",
    blocked: (first, more) =>
      `Erstellen nicht möglich: ${first}${more > 0 ? ` (und ${num(more)} weitere, siehe „Quelle“)` : ""}.`,
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index listet nicht die Dateien dieses Spiels" : state === "damaged" ? "ampr_emu.index ist nicht lesbar" : "Dieses Spiel hat keine ampr_emu.index"}: Die Konvertierung schreibt eine neue.`,
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
      zstd: { tagline: "Mit ampr-emu-ng", cells: ["Schnelleres Laden", "Bis zu halb so groß wie das Spiel"] },
      lz4: {
        tagline: "Mit ampr-emu-ng",
        cells: ["30–40 % schnelleres Laden, laut den Autoren", "Etwa halb so groß wie das Spiel, laut den Autoren"],
      },
      ffpkg: { tagline: "Empfohlen für ShadowMountPlus", cells: ["Volles Image-Tempo", FLAT] },
      exfat: { tagline: "Kompatibilität", cells: ["Volles Image-Tempo", FLAT] },
      ffpfs: { tagline: "Unkomprimiertes PFS", cells: ["Volles Image-Tempo", FLAT] },
      ffpfsc: {
        tagline: "Am kleinsten",
        cells: ["150–250 MB/s, kann ruckeln", "Hängt von der Kompressionsstufe ab, meist 30–60 % kleiner"],
      },
      folder: { tagline: "Normale Spieldateien", cells: ["Volles Laufwerkstempo", FLAT] },
    },
    compare: "Formate vergleichen",
    compareLabel: "Formatvergleich",
    format: "Format",
    credits: "Danksagungen",
    upstream: (app: string) =>
      `Angaben der Autoren: was die Autoren von ampr_emu und Lazy_AMPR berichten, nicht von ${app} gemessen; die Werte schwanken je nach Spiel.`,
    bundled: "ampr_emu von drakmor (GPL-3.0), ein Fork davon ist ampr-emu-ng. Forge liefert es nicht mehr mit: Es ersetzt das ampr_emu eines Spiels durch ampr-emu-ng, sofern die Einstellungen es nicht behalten. Quellcode:",
    lazy: "Die Packstrategie baut auf Lazy_AMPR von Nazky auf:",
    ng: "ZSTD- und LZ4-Pakete laufen auf ampr-emu-ng (GPL-3.0-or-later), der einzigen mitgelieferten AMPR-Runtime, unverändert. Quellcode:",
    logo: "Das Logo stammt von Xxcco:",
    unrar:
      ".rar-Archive werden mit der mitgelieferten UnRAR-Bibliothek von Alexander Roshal gelesen (nur Extrahieren, unter der UnRAR-Lizenz).",
    language: "Sprache",
    system: "System",
    notifications: "Nach Abschluss eines Auftrags benachrichtigen",
    notificationsHint: "Wird auf der Konsole angezeigt, wenn ein Erstell-, Pack- oder Übertragungsauftrag endet.",
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
    facts: "Spieldaten",
    explorer: "Spieldateien",
    howTitle: "So funktioniert es",
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
    archiveNote: "Archive können durchsucht, aber nicht bearbeitet werden. Um es zu ändern, erstellen Sie zuerst einen Ordner oder ein Image daraus.",
    rebuildTimePkg: (t) => `Neu erstellen komprimiert das ganze Spiel erneut: etwa ${t} bei ~26 MB/s auf einer PS5 (auf einem Computer schneller). Bearbeiten Sie stattdessen eine entpackte Kopie, um das zu vermeiden.`,
    rebuildTimePs5: (t) => `Neu erstellen schreibt das ganze Image erneut (etwa ${t} auf dieser PS5).`,
    rebuildTimeHost: (t) => `Neu erstellen schreibt das ganze Image erneut (etwa ${t} auf diesem Computer).`,
    rebuildTimeAny: (t) => `Neu erstellen schreibt das ganze Image erneut (etwa ${t} auf einer PS5).`,
    nasNote: "Spiele auf einem NAS lassen sich durchsuchen und extrahieren, aber nicht bearbeiten. Erstelle oder extrahiere das Spiel zuerst auf dieses Gerät, um es zu ändern.",
    patchNote: "Das ist ein Patch-Paket: Es enthält nur, was das Update geändert hat. Aufgeführt sind die Dateien, die es vollständig enthält; sie lassen sich durchsuchen und extrahieren, aber nicht bearbeiten.",
    peerNote: "Spiele auf einem anderen Forge lassen sich durchsuchen und extrahieren, aber nicht bearbeiten. Erstelle oder extrahiere das Spiel zuerst auf dieses Gerät, um es zu ändern.",
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
    alwaysRebuilt: "Images im Format ffpfsc und FPKG werden immer neu erstellt.",
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
    applyRunning: "Die Änderungen werden angewendet. Bearbeiten ist wieder möglich, wenn der Auftrag beendet ist (siehe „Warteschlange“).",
    applyFailed: "Die Änderungen wurden nicht angewendet:",
    applyRefused: "Die Änderungen lassen sich so nicht anwenden:",
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
    stepFailed: (s) => `${s}: fehlgeschlagen`,
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
      zstdPack: (level: string) => `ZSTD packen · ${level}`,
      level: (n: number) => `Stufe ${num(n)}`,
      zstdUnpack: "ZSTD entpacken",
      withRules: (tag) => `${tag} · eigene Regeln`,
    },
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
    outputPath: "Ausgabepfad",
    sourceDeleted: (v) => `Quelle gelöscht: ${v}`,
    sourceKept: (v) => `Quelle behalten: ${v}`,
    sourcePartlyDeleted: (v) => `Quelle teilweise gelöscht: ${v}`,
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
  // Storage text shared by Files.tsx, Packages.tsx, Edit.tsx and common.tsx (FreeSpace)
  library: {
    cantDelete: "Löschen nicht möglich:",
    deleted: (path) => `Gelöscht: ${path}`,
    storageTip: "Freier Speicher im internen Speicher der Konsole (/data)",
    free: (free, total) => (
      <>
        <b>{free}</b> von {total} frei
      </>
    ),
    cards: "Kachelansicht",
    list: "Listenansicht",
    remove: (n) => `${n} löschen`,
    deleting: (n) => `${n} wird gelöscht`,
    showAs: "Anzeigen als",
  },
  // Storage.tsx, Files.tsx: the Storage tab (File explorer | Package installer)
  storage: {
    views: "Speicheransicht",
    files: "Datei-Explorer",
    title: "Datei-Explorer",
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
    otherDevice: "„Verschieben“ bleibt auf einem Gerät: Der Bildschirm „Übertragung“ kopiert zwischen Geräten.",
    loadingDevices: "Geräte werden geladen",
    noDevice: "Kein Gerät verbunden",
    noDeviceHint: "Koppeln Sie eine PS5 oder einen Computer oder fügen Sie ein NAS hinzu, um seine Dateien hier zu verwalten.",
    openConnect: "„Verbinden“ öffnen",
    dropMove: (folder) => `Loslassen, um nach ${folder} zu verschieben`,
    cantCopy: "Kopieren nicht möglich:",
    copyNoFolder: "Öffnen Sie zuerst einen Ordner auf jenem Gerät.",
    copyOne: (name: string, to: string) => `${name} wird nach ${to} kopiert. Die Übertragung steht in der „Warteschlange“.`,
    copyMany: (n: number, to: string) =>
      plural(n, {
        one: `${num(n)} Element wird nach ${to} kopiert. Die Übertragung steht in der „Warteschlange“.`,
        other: `${num(n)} Elemente werden nach ${to} kopiert. Die Übertragung steht in der „Warteschlange“.`,
      }),
  },
  // build/ (Build's pack settings and checks), JobList.tsx's pack tags
  pack: {
    help: "So funktioniert Packen",
    patch: "Ein Patch-Paket lässt sich nicht packen.",
    additionalContent: "Zusatzinhalte (ohne eboot.bin) lassen sich nicht packen.",
    levels: { fast: "Schnell", medium: "Mittel", max: "Maximal" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Empfohlen. ZSTD 5: schnell, gute Größe." },
        medium: { text: "ZSTD 9: kleiner, langsamer." },
        max: { text: "ZSTD 19: am kleinsten; viel langsamer, mehr Speicher." },
      },
      lz4: {
        fast: { tone: "good", text: "Empfohlen. LZ4-Standard: am schnellsten." },
        medium: { text: "LZ4 HC 9: kleiner, langsamer." },
        max: { text: "LZ4 HC 12: am kleinsten, am langsamsten." },
      },
    },
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index ist veraltet" : state === "damaged" ? "ampr_emu.index ist beschädigt" : "Keine ampr_emu.index"}: Das Packen schreibt eine neue.`,
    rules: {
      title: "Eigene Regeln (TOML)",
      titleWith: (name) => `Eigene Regeln (TOML): ${name}`,
      info: "Ein Profil der Packstrategie im Format von ampr_emus ampr_pack.py und Lazy_AMPR legt fest, welche Dateien gepackt werden und welche einzeln bleiben. Die letzte passende Regel gilt; Systemdateien bleiben immer einzeln.",
      warn: "Falsche Regeln können ein Spiel hinterlassen, das nicht startet. Behalten Sie eine Kopie.",
      pickTitle: "Packstrategie (TOML-Datei)",
      clear: "Entfernen",
    },
  },
  // PackHelp.tsx
  packHelp: {
    title: "So funktioniert Packen",
    close: "Schließen",
    body: () => (
      <>
        <h3>ZSTD</h3>
        <ol>
          <li>Packt die Assets des Spiels in ZSTD-Volumes, in einen Ordner oder ein Image.</li>
          <li><b>Schnell</b> (ZSTD 5) ist flott; <b>Mittel</b> (9) und <b>Maximal</b> (19) sind kleiner, aber langsamer.</li>
          <li>Bei einem ZSTD-gepackten Spiel entpackt <b>Keine</b> es: Die normalen Dateien kommen zurück.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Manche Spiele starten eventuell nicht.</b> Bewahren Sie eine Kopie auf.</li>
          <li>Packt in LZ4-Volumes, in einen Ordner oder ein Image: <b>Schnell</b> (LZ4), <b>Mittel</b> (HC 9), <b>Maximal</b> (HC 12).</li>
          <li>Läuft auf ampr-emu-ng, wie ZSTD-Pakete.</li>
          <li>Bei einem LZ4-gepackten Spiel entpackt <b>Keine</b> es.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>Die einzige AMPR-Runtime, die Forge mitliefert. Sie liest ZSTD- und LZ4-Pakete; das Packen installiert sie in fakelib/.</li>
          <li>Das Erstellen ersetzt das ampr_emu eines Spiels durch sie, sofern die Einstellungen ampr_emu nicht behalten. Ein FPKG enthält keine Runtime: Der Loader der Konsole stellt sie bereit.</li>
          <li>Schalten Sie das fakelib-Update von ShadowMount+ aus, sonst ersetzt es die Runtime und das Spiel kann seine Pakete nicht lesen.</li>
        </ol>

        <h3>Packstrategie</h3>
        <ol>
          <li><b>Alles außer Systemdateien packen</b> (ZSTD und LZ4): jede Datei außer eboot.bin, Programmen und Modulen, sce_sys, sce_module, fakelib und .ini-Dateien (dlc_emu.ini bleibt bearbeitbar); ampr-emu-ng liefert den Rest, Audio und Video eingeschlossen.</li>
          <li>Beim Packen: Wählen Sie unter <b>Eigene Regeln (TOML)</b> ein Profil im Format von ampr_pack.py und Lazy_AMPR, auf diesem oder einem anderen Gerät.</li>
          <li>Die letzte passende Regel gilt; Systemdateien bleiben immer einzeln. Falsche Regeln können ein Spiel hinterlassen, das nicht startet.</li>
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
    netOnlyHere: "Archive und FPKG nur von diesem Gerät.",
  },
  // bookmarks.tsx: the bookmarked folders (Transfer and the File explorer)
  bookmarks: {
    label: "Lesezeichen",
    add: "Lesezeichen für diesen Ordner hinzufügen",
    remove: "Lesezeichen entfernen",
    removeNamed: (name: string) => `Lesezeichen ${name} entfernen`,
    full: "Höchstens 9 Lesezeichen",
  },
  // DeviceTabs.tsx
  deviceTabs: {
    add: "Gerät hinzufügen",
    offline: "Offline",
    offlineNamed: (name: string) => `${name} · Offline`,
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
    unsigned: (n: number) =>
      plural(n, {
        one: `${num(n)} ausführbare Datei ist nicht signiert, daher startet das Spiel nicht. Beim Konvertieren wird sie signiert.`,
        other: `${num(n)} ausführbare Dateien sind nicht signiert, daher startet das Spiel nicht. Beim Konvertieren werden sie signiert.`,
      }),
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
    traceRuntime: "Alte LZ4-Trace-Runtime",
    noRuntime: "Keine Runtime",
    packed: (kind) => `Gepackt (${kind})`,
    zstdKept: "ZSTD-Asset-Pakete: Eine Konvertierung behält sie",
    damaged: "Beschädigte Pakete",
    unpacked: "Ungepackt",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} in ${num(volumes)} ${plural(volumes, { one: "Volume", other: "Volumes" })}` +
      (percent === null ? "" : `, ${num(percent)} % der Größe`),
    index: {
      stale: "ampr_emu.index veraltet: Eine Konvertierung erneuert sie",
      damaged: "ampr_emu.index beschädigt: Eine Konvertierung schreibt sie neu",
      missing: "keine ampr_emu.index: Eine Konvertierung schreibt eine",
    },
    runtimeAction: (a: "install" | "replace" | "keep" | "keepIfPacks") =>
      a === "install"
        ? "eine Konvertierung installiert das mitgelieferte ampr-emu-ng"
        : a === "replace"
          ? "eine Konvertierung ersetzt die Runtime durch das mitgelieferte ampr-emu-ng"
          : a === "keep"
            ? "eine Konvertierung behält es (Einstellungen)"
            : "eine Konvertierung behält es, wenn es LZ4-Pakete liest, sonst ersetzt sie es durch ampr-emu-ng",
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
    wrong: "Das Passwort ist nicht korrekt. Prüfen Sie es und versuchen Sie es erneut.",
    label: "Passwort",
    cancel: "Abbrechen",
    unlock: "Entsperren",
    checking: "Prüfung",
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
    deleteSource: "Quelle löschen",
    deleteSourceHint: "Nicht rückgängig zu machen. Die Quelle wird erst nach einer erfolgreichen Konvertierung gelöscht.",
    deleteSourceNas: "Ein Spiel auf einem NAS kann von hier aus nicht gelöscht werden.",
    separator: "Ein Name darf kein Ordnertrennzeichen enthalten; den Ordner ändern Sie mit „Ändern“.",
    badName: "Ein Name darf nicht leer sein, nicht mit einem Punkt beginnen, nicht auf Punkt oder Leerzeichen enden und keine Steuerzeichen enthalten.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Zu lang: ${bytes} Byte, höchstens ${max} (UTF-8).`
        : `Zu lang: ${bytes} Byte vor der Endung, höchstens ${max} (UTF-8).`,
    pkgExt: (ext) => `Endet auf .${ext}, aber FPKG wird als .pkg gespeichert: Die Endung bestimmt den Treiber.`,
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
  },
  // Formats.tsx (About's Output names) and target.tsx's name pattern: generated output names
  naming: {
    title: "Ausgabenamen",
    parts: { title: "Titel", title_id: "Titel-ID", version: "Version", pack: "Pack-Art", region: "Region", fw: "Firmware" },
    template: "Namensvorlage",
    partsLabel: "Teil einfügen",
    insert: (part: string) => `${part} einfügen`,
    inUse: (part) => `${part} ist schon in der Vorlage`,
    example: "Beispiel",
    length: "Länge",
    bytes: (n, max) => `${n} von ${max} Bytes.`,
    room: (n) => plural(n, { one: `Für den Titel bleibt ${num(n)} Byte.`, other: `Für den Titel bleiben ${num(n)} Bytes.` }),
    noRoom: "Für den Titel bleibt kein Platz: Er entfällt.",
    versionOut: "Die Version entfällt, um Platz zu schaffen.",
    cut: "Der Name wird bei 63 Bytes abgeschnitten.",
    idOnly: "Der Text der Vorlage füllt die 63 Bytes, deshalb benennt die Titel-ID die Ausgabe.",
    errors: {
      long: (max: number) => `Eine Vorlage hat höchstens ${max} Bytes.`,
      open: "Zu einer { fehlt die schließende }.",
      close: "Zu einer } fehlt die öffnende {.",
      unknown: (token: string) => `{${token}} ist kein Namensteil: Verwenden Sie Titel, Titel-ID, Version, Pack-Art, Region oder Firmware.`,
      twice: (token) => `{${token}} steht zweimal in der Vorlage: Verwenden Sie jeden Teil nur einmal.`,
      char: (c: string) => `Ein Name darf kein ${c} enthalten.`,
      control: "Ein Name darf kein Steuerzeichen enthalten.",
      needsName: "Ein Name braucht Titel oder Titel-ID.",
    },
    notSaved: "Nicht gespeichert: Namen folgen weiter der letzten gültigen Vorlage.",
    reset: "Zurücksetzen",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} ist verfügbar`,
    update: "Aktualisieren",
    later: "Später",
    updating: "Wird aktualisiert…",
    updatingNote: "Die neue Version wird heruntergeladen. Diese Seite lädt sie danach von selbst neu.",
    downloading: "Der Download öffnet sich im Browser: Ersetzen Sie diese Kopie durch die heruntergeladene Datei (auf dem Mac ziehen Sie die App aus der .dmg in „Programme“).",
    downloadingCli: "Der Download öffnet sich im Browser: Ersetzen Sie die ps5-dump-forge-CLI durch die heruntergeladene Datei oder führen Sie ps5-dump-forge update --install auf dem Computer aus, auf dem sie läuft.",
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
    thisBrowser: "Dieser Browser",
    uploadPick: "Dateien zum Hochladen wählen",
    uploading: (name, i, n) => `${name} wird hochgeladen (${num(i)} von ${num(n)})`,
    uploaded: "Upload abgeschlossen",
    uploadSkipped: (names) => `Nicht hochgeladen, dort gibt es schon einen Ordner oder Link mit diesem Namen: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `${num(done)} von ${num(n)} Dateien wurde hochgeladen und bleibt auf diesem Gerät.`,
        other: `${num(done)} von ${num(n)} Dateien wurden hochgeladen und bleiben auf diesem Gerät.`,
      }),
    cantUpload: "Hochladen nicht möglich:",
    send: "Senden",
    downloadHere: "Von diesem Gerät herunterladen",
    wholeRoot: "Ein ganzes Gerät, eine Freigabe oder ein Laufwerk kann nicht gesendet werden: Wählen Sie einen Ordner darin.",
    helpPhone: "Scannen Sie den QR-Code, um diese Seite auf einem Telefon zu öffnen. Dort lädt „Übertragung“ die Dateien des Telefons auf dieses Gerät hoch und die Dateien dieses Geräts auf das Telefon herunter.",
    help: [
      "Verbinden verknüpft Dump Forge auf Ihrer PS5 und Ihren Computern im selben Netzwerk.",
      "Zum Koppeln starten Sie die „Automatische Suche“ und wählen „Koppeln“ unter „In der Nähe“ oder geben eine Adresse unter „Gerät hinzufügen“ ein und erlauben es dann auf dem anderen Gerät. Erlauben Sie es nur, wenn beide Bildschirme dieselbe Prüfnummer zeigen.",
      "Ein NAS (SMB, FTP, NFS) wird genauso hinzugefügt. Gekoppelte Geräte und NAS sind Spielquellen und Ziele; ihre Spiele werden dort nicht bearbeitet.",
      "„Übertragung“ im Seitenmenü kopiert Dateien und Ordner zwischen ihnen. Ein Name, der dort schon existiert, wird erst nach Ihrer Bestätigung ersetzt.",
    ],
    helpSecurity: "Nichts, was zwischen Geräten gesendet wird, ist verschlüsselt: Jeder in Ihrem Netzwerk kann es mitlesen. Ein gespeichertes NAS-Passwort liegt unverschlüsselt auf diesem Gerät. Nutzen Sie Verbinden nur im Heimnetz.",
    helpSpeed: "„Geschwindigkeitstest“ in der Zeile eines gekoppelten Geräts oder in seinen Details sendet 128 MiB dorthin und holt 128 MiB zurück, ohne etwas auf die Festplatte zu schreiben, und zeigt die Netzwerkgeschwindigkeit in beide Richtungen.",
    speedTest: "Geschwindigkeitstest",
    speedTestWith: (name) => `Geschwindigkeitstest mit ${name}`,
    speedTesting: "Wird getestet",
    speedResult: (up, down) => `Upload ${up} MB/s · Download ${down} MB/s`,
    speedFailed: "Geschwindigkeitstest fehlgeschlagen:",
    speedNote: "Sendet 128 MiB und holt 128 MiB zurück; nichts wird auf die Festplatte geschrieben.",
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
    started: (name: string) => `${name} wird installiert. Der Auftrag steht in der „Warteschlange“.`,
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
    sent: (device: string) => `${device} installiert das Paket. Der Auftrag steht in der „Warteschlange“.`,
  },
  // Build's and Edit's Restore metadata online switch (build/options.tsx, Edit.tsx)
  metadata: {
    tag: "Metadaten",
    cant: "Metadaten nicht wiederhergestellt:",
    label: "Metadaten online wiederherstellen",
    buildHint: "Lädt vor dem Build param.json, Bilder, Ton und PlayGo-Tabellen dieser Version von Sonys Update-Server und legt sie dort in die Ausgabe, wo sie abweichen. Das Spiel selbst bleibt unverändert. Braucht Internet; ohne läuft der Build so weiter.",
    buildSummary: "Legt Sonys Metadaten in die Ausgabe (das Spiel bleibt unverändert)",
    editHint: "Wendet param.json, Bilder, Ton und PlayGo-Tabellen dieser Version von Sonys Update-Server mit deinen Änderungen auf dieses Spiel an, wo sie abweichen: ein Ordner wird direkt geändert, ein exfat oder ffpkg an Ort und Stelle, ein ffpfs, ffpfsc oder FPKG neu gebaut. Braucht Internet; schlägt der Download fehl, bleibt alles unverändert.",
    editOnly: "Sonys Metadaten für diese Version, auf dieses Spiel angewendet",
    editAlso: "Sonys Metadaten für diese Version werden auch angewendet, vorher heruntergeladen.",
    editConfirm: "Sonys Metadaten anwenden?",
    editConfirmText: "Die Metadaten werden zuerst heruntergeladen; schlägt das fehl, bleibt alles unverändert.",
  },
  // App.tsx: the side rail
  rail: {
    build: "Erstellen",
    settings: "Einstellungen",
    queue: "Warteschlange",
    transfer: "Übertragung",
    collapse: "Menü einklappen",
    expand: "Menü ausklappen",
  },
  // Queue.tsx
  queue: {
    empty: "Noch keine Aufträge. Erstellungen, Bearbeitungen, Übertragungen und Installationen erscheinen hier.",
    counts: (running: number, queued: number, failed: number, done: number): string =>
      [
        running && `${num(running)} laufen`,
        queued && `${num(queued)} warten`,
        failed && `${num(failed)} fehlgeschlagen`,
        done && `${num(done)} fertig`,
      ]
        .filter(Boolean)
        .join(", ") || "Nichts anzuzeigen",
    retryFailed: (n: number): string => (n > 0 ? `Fehlgeschlagene wiederholen (${num(n)})` : "Fehlgeschlagene wiederholen"),
    retrying: (i: number, n: number) => `Wiederhole ${num(i)} von ${num(n)}`,
    clearFinished: "Beendete entfernen",
    filters: { all: "Alle", running: "Laufend", failed: "Fehlgeschlagen", done: "Fertig" },
    filterLabel: "Anzeigen",
    noneShown: "Keine Aufträge passen zu diesem Filter.",
  },
  // Settings.tsx
  settings: {
    appearance: "Darstellung",
    theme: "Design",
    themeHint: "Dunkel passt zum Fernseher, hell zu einem hellen Raum.",
    dark: "Dunkel",
    light: "Hell",
    scheme: "Farbschema",
    schemeHint: "Die Akzentfarbe in der ganzen App.",
    schemes: { abyss: "Tiefsee", ember: "Glut", moss: "Moos", cobalt: "Kobalt", dusk: "Dämmerung", rose: "Rose", sand: "Sand", volt: "Volt", mono: "Mono" },
    advanced: "Erweiterte Optionen",
    advancedOpen: "Erweiterte Optionen immer anzeigen",
    advancedHint:
      "Zeigt alle Pakete und Formate ohne „Mehr“ und hält alle eingeklappten Bereiche offen. Schalten Sie es aus, damit Erstellen kurz bleibt.",
    bundled: (version: string) => `${version} (mitgeliefert)`,
    replaceAmprEmu: "ampr_emu durch ampr-emu-ng ersetzen",
    languageHint: "Die Sprachen des Systems, bis Sie eine wählen.",
    notificationsGroup: "PS5",
  },
  // JobList.tsx: a finished job's result
  result: {
    file: "Ausgabe",
    type: "Typ",
    size: "Größe",
    files: "Dateien",
    time: "Dauer",
    verified: "Geprüft",
    verifiedFast: "Geprüft (schnell)",
    verifiedFull: "Geprüft (vollständig)",
    notVerified: "Keine Prüfung gemeldet",
    checks: "Prüfungen:",
    copy: "Details kopieren",
    copied: "Kopiert",
    copyFailed: "Hier nicht kopierbar",
    ofSource: (pct: number) => `${num(pct, 1)} % der Spieldateien`,
    filesN: (n: number) => `${num(n)} ${plural(n, FILES)}`,
    to: "geschrieben nach",
    took: (d: string) => `dauerte ${d}`,
    kinds: {
      pkg: "FPKG: auf einer PS5 mit FPKG-Unterstützung installierbar",
      exfat: "exfat-Image: ShadowMount+ bindet es beschreibbar ein",
      ffpkg: "ffpkg-Image (UFS2): ShadowMount+ bindet es beschreibbar ein",
      ffpfs: "ffpfs-Image (PFS): ShadowMount+ bindet es ein",
      ffpfsc: "ffpfsc-Image (komprimiertes PFS): ShadowMount+ bindet es schreibgeschützt ein",
      folder: "Spielordner",
    } as Record<string, string | undefined>,
    zstdFolder: "Ordner mit ZSTD-Paketen",
    lz4Folder: "Ordner mit LZ4-Paketen",
    withPacks: (image: string, kind: string) => `${image}, Dateien in ${kind}-Paketen`,
    edited: (what: string) => `${what}, direkt geändert`,
    rebuilt: (what: string) => `${what}, neu erstellt`,
    extracted: "Extrahierte Dateien",
    transferred: "Kopierte Dateien",
    installed: "Paket an den Installer der PS5 übergeben",
  },
  // JobList.tsx: the log panel
  log: {
    lines: (n: number, of?: number): string =>
      of === undefined
        ? `${num(n)} ${plural(n, { one: "Zeile", other: "Zeilen" })}`
        : `${num(n)} von ${num(of)} ${plural(of, { one: "Zeile", other: "Zeilen" })}`,
    filter: "Filter",
    follow: "Automatisch scrollen",
    copy: "Kopieren",
    copied: "Kopiert",
    copyFailed: "Kopieren fehlgeschlagen",
    save: "Speichern",
    clear: "Leeren",
    cleared: "Geleert",
  },
  // common.tsx: the source card's fact chips
  sourceChips: {
    sdk: (v: string) => `SDK ${v}`,
    firmware: (v: string) => `Firmware ${v}+`,
  },
  // prefs.tsx: Advanced's badge
  advanced: {
    changed: "Geändert",
    defaults: "Standard",
  },
  // build/*: the Build screen
  build: {
    summary: "Zusammenfassung",
    addToQueue: "Zur Warteschlange hinzufügen",
    queued: "Zur Warteschlange hinzugefügt",
    openQueue: "Warteschlange öffnen",
    space: (free: string, dir: string, needed: string) => (
      <>
        {free} frei in <span className="path">{dir}</span>, etwa {needed} nötig
      </>
    ),
    needed: (needed: string) => `Etwa ${needed} nötig`,
    spaceRaw: "Der Platz wird wie ohne Kompression geprüft.",
    estimate: "Schätzung",
    advanced: "Erweiterte Optionen",
    options: "Optionen",
    fastVerify: "Schnelle Prüfung: Jede Struktur wird geprüft, eine Stichprobe der Daten zurückgelesen.",
    fullVerify: "Vollständige Prüfung: Jedes Byte wird zurückgelesen.",
    deleted: "Die Quelle wird nach einer erfolgreichen Erstellung gelöscht. Kein Rückgängig.",
    zstdBackport:
      "Die Assets dieses Spiels sind mit ZSTD gepackt und sein ZSTD-Index listet den Backport: Erstellen Sie es zuerst einmal mit „Pakete: Keine“ und entfernen Sie dann den Backport.",
    lz4Backport:
      "Die Assets dieses Spiels sind mit LZ4 gepackt und sein LZ4-Manifest listet den Backport: Erstellen Sie es zuerst einmal mit „Pakete: Keine“ und entfernen Sie dann den Backport.",
    backportLeftOut: (n: number) =>
      plural(n, {
        one: `${num(n)} Backport-Bibliothek bleibt aus fakelib/ heraus.`,
        other: `${num(n)} Backport-Bibliotheken bleiben aus fakelib/ heraus.`,
      }),
    packsTitle: "Ziel",
    packs: "Pakete",
    more: "Mehr",
    moreTip: "Weitere Formate",
    cards: {
      folder: "Normale Dateien, überallhin kopierbar, ShadowMount+ startet es",
      pkg: "Wird auf der PS5 wie ein Spielpaket installiert",
      exfat: "Wird von ShadowMount+ eingebunden und öffnet sich am PC",
      ffpkg: "UFS2-Image für ShadowMount+, beschreibbares Einbinden möglich",
      ffpfs: "PFS-Image, unkomprimiert, nur ASCII-Dateinamen",
      ffpfsc: "Kleinstes Image: komprimiert, immer schreibgeschützt eingebunden",
      lz4: "Assets mit LZ4 gepackt, normaler Ordner",
    },
    zstdFolderCard: "Assets mit ZSTD gepackt, normaler Ordner",
    packCards: {
      nonePlain: "Die Dateien des Spiels, wie sie sind",
      unpack: "Schreibt die Pakete als normale Dateien zurück",
      keepZstd: "Die ZSTD-Pakete bleiben Byte für Byte erhalten",
      keepLz4: "Die LZ4-Pakete bleiben Byte für Byte erhalten",
      zstd: "Kleineres Spiel, schnelleres Laden, mit ampr-emu-ng",
      repackZstd: "Entpackt die LZ4-Pakete und packt neu mit ZSTD",
      lz4: "Packt Assets mit LZ4, mit ampr-emu-ng",
      repackLz4: "Entpackt die ZSTD-Pakete und packt neu mit LZ4",
    },
    naming: "Ausgabe: Der Name wird erzeugt",
    output: (name: string, dir: string) => (
      <>
        Ausgabe: <b className="path">{name}</b> in <span className="path">{dir}</span>
      </>
    ),
    choices: { none: "Keine", zstd: "ZSTD", lz4: "LZ4" },
    levelLabel: (level: string, engine: string, n: number) => `${level}, ${engine} ${n}`,
    levelTip: (engine: string, n: number, note: string) => `${engine} ${n}. ${note}`,
    sumKeepZstd: "Die ZSTD-Pakete bleiben Byte für Byte erhalten.",
    sumKeepLz4: "Die LZ4-Pakete bleiben Byte für Byte erhalten.",
    sumKeepPlain: "Es werden keine Pakete erstellt: Die Dateien des Spiels werden konvertiert, wie sie sind.",
    sumUnpack: (engine: string) => `Die ${engine}-Pakete werden als normale Dateien zurückgeschrieben.`,
    sumPack: (engine: string, level: string) => `Die Dateien des Spiels werden in ${engine}-Volumes gepackt (${level}).`,
    sumRepack: (from: string, to: string, level: string) => `Die ${from}-Pakete werden entpackt und mit ${to} neu gepackt (${level}).`,
    sumRules: (name: string) => `Die eigenen Regeln ${name} legen fest, welche Dateien gepackt werden.`,
    sumInside: (format: string, level: number) => `Innen: ${format}, zlib-Stufe ${level}.`,
    ngInstalled: "ampr-emu-ng wird unter fakelib/libSceAmpr.sprx installiert.",
    ngReplaced: (old: string) => `${old} des Spiels wird durch ampr-emu-ng ersetzt.`,
    ngReplacedTrace: "Die alte Trace-Runtime des Spiels wird durch ampr-emu-ng ersetzt.",
    ngKept: (old: string) => `${old} des Spiels bleibt erhalten.`,
    ngUpdated: (v: string) => `ampr-emu-ng${v} des Spiels wird durch die mitgelieferte ersetzt.`,
    ngSame: (v: string) => `ampr-emu-ng${v} des Spiels bleibt erhalten.`,
    pkgNoRuntime: "Die AMPR-Runtime bleibt aus dem FPKG heraus: Der Loader der Konsole stellt sie bereit.",
    recommended: "Empfohlen",
    resultTitle: "Etwa, nach dem Erstellen",
    resultNow: "Spiel jetzt",
    resultCustomRules: "eigene Regeln: keine Schätzung",
    backportTitle: "Backport",
    backportNone: "Dieses Spiel hat keinen Backport.",
    backportPlain: "Nur bei einer einfachen Konvertierung: ohne Packen oder Entpacken.",
  },
  // TransferPane.tsx: one device's pane of the Transfer screen
  transfer: {
    pane: (device) => `Dateien auf ${device}`,
    roots: "Laufwerke",
    location: "Ort",
    up: "Nach oben",
    upTip: "Einen Ordner nach oben",
    refresh: "Aktualisieren",
    filter: "Nach Name filtern",
    filterLabel: "Diesen Ordner nach Name filtern",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "Ordner", other: "Ordner" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => plural(n, { one: `${num(n)} Treffer`, other: `${num(n)} Treffer` }),
    firstOf: (max, n) => `erste ${num(max)} von ${num(n)}`,
    truncated: "Dieser Ordner enthält mehr als 10.000 Einträge: Nur die ersten 10.000 werden angezeigt.",
    name: "Name",
    size: "Größe",
    select: (name) => `${name} auswählen`,
    open: (name) => `${name} öffnen`,
    selected: (n) => plural(n, { one: `${num(n)} Element ausgewählt`, other: `${num(n)} Elemente ausgewählt` }),
    clear: "Aufheben",
    clearTip: "Auswahl aufheben",
    dragItems: (n) => plural(n, { one: `${num(n)} Element`, other: `${num(n)} Elemente` }),
    dropHere: (path) => `Ablegen, um es nach ${path} zu senden`,
    notHere: "Öffnen Sie einen Ordner, um Dateien hierher zu senden.",
    emptyFolder: "Dieser Ordner ist leer.",
    noMatch: (f) => `Hier passt nichts zu „${f}“.`,
    noRoots: "Keine Laufwerke zum Durchsuchen verfügbar.",
    cantList: "Dieser Ordner kann nicht geöffnet werden:",
    loading: "Wird geladen",
    link: "Link",
    linkTip: "Ein Link: Er lässt sich öffnen, aber nicht auswählen",
    mounted: "Eingehängt",
    mountedTip: "Von ShadowMount+ eingehängt: Beenden Sie das Spiel und schalten Sie ShadowMount+ aus, um es auszuwählen",
    withTitle: "Übertragen mit",
    loadingDevices: "Geräte werden geladen",
    noneTitle: "Kein Gerät für eine Übertragung",
    noneHint: "Koppeln Sie unter „Verbinden“ ein weiteres Dump Forge oder fügen Sie ein NAS hinzu, oder schalten Sie ein gespeichertes ein.",
    openConnect: "„Verbinden“ öffnen",
    deviceOffline: (device) => `${device} ist offline. Seine Dateien erscheinen wieder, sobald es zurück ist.`,
    copyTo: (n, device) =>
      plural(n, { one: `${num(n)} Element nach ${device} kopieren`, other: `${num(n)} Elemente nach ${device} kopieren` }),
    copyNoFolder: (device) => `Öffnen Sie einen Ordner auf ${device}, in den kopiert werden soll.`,
    installOn: (device) => `Auf ${device} installieren`,
    dropFiles: "Dateien hier ablegen, um sie hochzuladen",
    uploadInto: (folder) => `Sie landen in ${folder}, dem Ordner, der auf diesem Gerät angezeigt wird.`,
    uploadNoFolder: "Öffnen Sie einen Ordner auf diesem Gerät, in den hochgeladen werden soll.",
    foldersLeftOut: (names) => `Ordner werden nicht hochgeladen, nur Dateien: ${names}.`,
    downloadFiles: (n) => plural(n, { one: `${num(n)} Datei herunterladen`, other: `${num(n)} Dateien herunterladen` }),
    downloadNone: "Wählen Sie Dateien auf diesem Gerät aus, um sie herunterzuladen (Ordner werden nicht heruntergeladen).",
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default de;
