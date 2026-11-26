// Italian. Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Terms: traccia/tracce (the recorded traces), tracciamento (tracing), runtime (m.),
// impacchettare/spacchettare, applicare/rimuovere la patch, operazione (job), sorgente (source).

import { dotted, num, plural } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "Pari alla dimensione del gioco";
const FILES = { one: "file", many: "file", other: "file" };
const CHECKS = { one: "controllo", many: "controlli", other: "controlli" };

const it: Messages = {
  // App.tsx
  app: {
    screens: "Schermate",
    active: (n) => `, ${num(n)} ${plural(n, { one: "attiva", many: "attive", other: "attive" })}`,
    edit: "Modifica",
    storage: "Archiviazione",
    connect: "Connetti",
    disconnected: "Non connesso",
    connected: (n: number) => plural(n, { one: `${num(n)} dispositivo`, many: `${num(n)} dispositivi`, other: `${num(n)} dispositivi` }),
    about: "Informazioni",
    offline: "Non in linea, nuovo tentativo…",
    stop: "Arresta",
    stillRunning: (n) =>
      n === 1
        ? "Un'operazione è ancora in corso"
        : `${num(n)} ${plural(n, { one: "operazione è ancora in corso", many: "operazioni sono ancora in corso", other: "operazioni sono ancora in corso" })}`,
    job: (id) => `Operazione ${id}`,
    quitNote: (web, n) =>
      web
        ? n === 1
          ? "Arrestando Dump Forge, l'operazione viene interrotta e l'output incompleto viene eliminato."
          : "Arrestando Dump Forge, le operazioni vengono interrotte e l'output incompleto viene eliminato."
        : n === 1
          ? "Uscendo, l'operazione viene interrotta e l'output incompleto viene eliminato."
          : "Uscendo, le operazioni vengono interrotte e l'output incompleto viene eliminato.",
    keepRunning: "Continua",
    stopAndQuit: (n) => (n === 1 ? "Interrompi ed esci" : "Interrompi tutto ed esci"),
    stoppingJobs: "Interruzione delle operazioni e pulizia…",
    stoppingForge: "Arresto di Dump Forge…",
    quitting: "Uscita da PS5 Dump Forge…",
    couldntStop: "Impossibile arrestare Dump Forge",
    close: "Chiudi",
    tryAgain: "Riprova",
    stopped: "Dump Forge è stato arrestato",
    startAgain: (ps5) => (ps5 ? "Per continuare, riapri il riquadro Dump Forge sulla PS5." : "Per continuare, esegui di nuovo lo stesso comando ps5-dump-forge serve."),
    confirmStop: (web: boolean) => (web ? "Arrestare Dump Forge?" : "Uscire da PS5 Dump Forge?"),
    stopNote: (web, ps5) => (!web ? "Nessuna attività in corso." : ps5 ? "Per riavviarlo, apri il riquadro Dump Forge sulla PS5." : "Per riavviarlo, esegui ancora lo stesso comando ps5-dump-forge serve."),
    peerBusy: "Un dispositivo abbinato sta leggendo o scrivendo file su questo computer. Uscendo, il suo trasferimento si interrompe.",
    quit: "Esci",
    github: (name) => `${name} su GitHub`,
    queued: "in coda",
    progress: (percent, left) => `${percent}% completato${left ? `, ${left} al termine` : ""}`,
  },
  // build/ (Build's convert settings and checks)
  convert: {
    targetFormat: "Formato",
    alreadyFolder: "la sorgente è già una cartella",
    patchBlocked: "Impossibile creare: è un pacchetto di patch, che contiene solo ciò che l’aggiornamento ha cambiato. Aprilo in “Modifica” per estrarne i file.",
    openInEdit: "Apri in “Modifica”",
    noContentId: "Impossibile creare come FPKG: il param.json di questo gioco non ha un Content ID, che un pacchetto richiede.",
    blocked: (first, more) =>
      `Impossibile creare: ${first}${more > 0 ? ` (e ${num(more)} ${plural(more, { one: "altro", many: "altri", other: "altri" })}, vedi “Sorgente”)` : ""}.`,
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index non elenca i file di questo gioco" : state === "damaged" ? "ampr_emu.index non è leggibile" : "Questo gioco non ha ampr_emu.index"}: la conversione ne scrive uno nuovo.`,
    removeBackport: "Rimuovi backport",
    removeHint: (libs, emulators, other) =>
      `Esclude ${num(libs)} ${plural(libs, { one: "libreria", many: "librerie", other: "librerie" })} di backport da fakelib/` +
      (emulators.length === 0
        ? other
          ? "; le altre librerie homebrew restano."
          : "."
        : other
          ? `; ${plural(emulators.length, { one: "l'emulatore", many: "gli emulatori", other: "gli emulatori" })} (${emulators.join(", ")}) e le altre librerie homebrew restano.`
          : `; ${plural(emulators.length, { one: "l'emulatore", many: "gli emulatori", other: "gli emulatori" })} (${emulators.join(", ")}) ${plural(emulators.length, { one: "resta", many: "restano", other: "restano" })}.`),
    cantRemove: (why) => `Impossibile rimuovere il backport: ${why}`,
    compare: "Confronta formati",
    build: "Crea",
    likelyFail: (fmt) => (
      <>
        Probabilmente non riuscirà come <b>{fmt}</b>: non accetta i nomi di file elencati in “Sorgente”.
      </>
    ),
    collides: "Un'operazione sta già scrivendo questo file: una nuova creazione non riuscirebbe. Scegli un altro nome.",
    already: (fmt) => (
      <>
        Questo gioco è già in creazione come <b>{fmt}</b>; creandolo di nuovo ottieni una seconda copia.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "Buono", ok: "Compromesso", bad: "Svantaggio" },
    columns: ["Velocità su console", "Spazio su disco"],
    rows: {
      pkg: {
        tagline: "Il migliore, se il firmware lo consente",
        cells: ["Nativa, piena velocità", "Dal 30 al 60% in meno, a seconda del gioco"],
      },
      zstd: { tagline: "Con ampr-emu-ng", cells: ["Caricamento più rapido", "Fino alla metà delle dimensioni del gioco"] },
      lz4: {
        tagline: "Con ampr-emu-ng",
        cells: ["Caricamenti più rapidi del 30–40%, secondo gli autori", "Circa metà del gioco, secondo gli autori"],
      },
      ffpkg: { tagline: "Consigliato per ShadowMountPlus", cells: ["Piena velocità dell'immagine", FLAT] },
      exfat: { tagline: "Compatibilità", cells: ["Piena velocità dell'immagine", FLAT] },
      ffpfs: { tagline: "PFS non compresso", cells: ["Piena velocità dell'immagine", FLAT] },
      ffpfsc: {
        tagline: "Il più piccolo",
        cells: ["150–250 MB/s, possibili scatti", "Dipende dal livello di compressione, di solito dal 30 al 60% in meno"],
      },
      folder: { tagline: "File del gioco così come sono", cells: ["Piena velocità dell'unità", FLAT] },
    },
    compare: "Confronta formati",
    compareLabel: "Confronto tra formati",
    format: "Formato",
    credits: "Crediti",
    upstream: (app: string) =>
      `Dati degli autori: quanto riportano gli autori di ampr_emu e Lazy_AMPR, non misurato da ${app}; variano da gioco a gioco.`,
    bundled: "ampr_emu di drakmor (GPL-3.0), di cui ampr-emu-ng è un fork. Forge non lo include più: sostituisce l'ampr_emu di un gioco con ampr-emu-ng, a meno che “Impostazioni” non lo mantenga. Codice sorgente:",
    lazy: "La strategia di impacchettamento si basa su Lazy_AMPR di Nazky:",
    ng: "I pacchetti ZSTD e LZ4 funzionano con ampr-emu-ng (GPL-3.0-or-later), l'unico runtime AMPR incluso, senza modifiche. Sorgente:",
    logo: "Il logo è di Xxcco:",
    unrar:
      "Gli archivi .rar vengono letti con la libreria UnRAR inclusa, di Alexander Roshal (solo estrazione, con la licenza UnRAR).",
    language: "Lingua",
    system: "Sistema",
    notifications: "Avvisa al termine di un'attività",
    notificationsHint: "Appare sulla console al termine di un'attività di creazione, impacchettamento o trasferimento.",
    notificationsFailed: "Impossibile modificare le notifiche.",
  },
  // Edit.tsx
  edit: {
    details: "Dettagli",
    deleting: "Eliminazione della sorgente",
    delete: "Elimina la sorgente",
    deleted: "Eliminato",
    filter: "Filtra per percorso",
    filterLabel: "Filtra i file per percorso",
    firstOf: (max, n) => plural(max, { one: `primo di ${num(n)}`, many: `primi ${num(max)} di ${num(n)}`, other: `primi ${num(max)} di ${num(n)}` }),
    fileCount: (n) => `${num(n)} ${plural(n, FILES)}`,
    noMatch: (f) => `Nessun file corrisponde a “${f}”.`,
    noListing: "L'elenco dei file dell'archivio non è supportato.",
    noParam: "Nessun sce_sys/param.json.",
    reader: "Lettore",
    leftovers: "File e cartelle rimasti",
    found: (n) => `${num(n)} ${plural(n, { one: "trovato", many: "trovati", other: "trovati" })}`,
    leftoversNote: (app) =>
      `Output incompleti, di solito lasciati da operazioni interrotte. Rimuovi solo quelli che riconosci, e solo quando non ci sono operazioni in corso; ${app} non li elimina mai da solo.`,
    checked: (n) => `Cartelle controllate (${num(n)})`,
    facts: "Dati del gioco",
    explorer: "File del gioco",
    howTitle: "Come funziona",
    how: (app) => [
      "Apri la **cartella** o l'**immagine** di un gioco, poi modificala come una cartella su una chiavetta USB: **aggiungi e sostituisci file** (trascinali qui o usa “Aggiungi”), **rinomina ed elimina**.",
      `Non cambia nulla **finché non premi “Applica”**: le modifiche si accumulano qui, poi ${app} le scrive tutte insieme direttamente nell'immagine o nella cartella e controlla il risultato. **Non è un'unità montata**: ${app} tiene traccia delle tue modifiche e le applica da solo.`,
      `Quando le modifiche ci stanno, vanno **direttamente nell'immagine**; quando non ci stanno (niente più spazio all'interno, o un formato che non le accetta), ${app} crea accanto una nuova immagine con le tue modifiche e la mette al suo posto quando supera la verifica. In ogni caso, se qualcosa va storto, l'immagine resta com'era.`,
      "Gli archivi si possono sfogliare, ma non modificare.",
    ],
    location: "Posizione nel gioco",
    root: "Gioco",
    up: "Su",
    upTip: "Su di una cartella",
    name: "Nome",
    size: "Dimensione",
    actions: "Azioni",
    items: (n) => `${num(n)} ${plural(n, { one: "elemento", other: "elementi" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "cartella", other: "cartelle" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "corrispondenza", other: "corrispondenze" })}`,
    emptyFolder: "Questa cartella è vuota.",
    open: (name) => `Apri ${name}`,
    archiveNote: "Gli archivi si possono sfogliare, ma non modificare. Per cambiarlo, crea prima una cartella o un'immagine a partire da esso.",
    rebuildTimePkg: (t) => `Ricostruisci comprime di nuovo l'intero gioco: circa ${t} a ~26 MB/s su una PS5 (più veloce su un computer). Modifica invece una copia spacchettata per evitarlo.`,
    rebuildTimePs5: (t) => `Ricostruisci riscrive l'intera immagine (circa ${t} su questa PS5).`,
    rebuildTimeHost: (t) => `Ricostruisci riscrive l'intera immagine (circa ${t} su questo computer).`,
    rebuildTimeAny: (t) => `Ricostruisci riscrive l'intera immagine (circa ${t} su una PS5).`,
    nasNote: "I giochi su un NAS si possono sfogliare ed estrarre, ma non modificare. Per modificarlo, crealo o estrailo prima su questo dispositivo.",
    patchNote: "È un pacchetto di patch: contiene solo ciò che l’aggiornamento ha cambiato. Sono elencati i file che contiene per intero; si possono sfogliare ed estrarre, ma non modificare.",
    peerNote: "I giochi su un altro Forge si possono sfogliare ed estrarre, ma non modificare. Per modificarlo, crealo o estrailo prima su questo dispositivo.",
    addFiles: "Aggiungi file",
    addFolder: "Aggiungi cartella",
    pickAddFiles: "Scegli i file da aggiungere",
    pickAddFolder: "Scegli la cartella da aggiungere",
    peerRoot: "Non si può aggiungere un intero dispositivo: scegli una cartella al suo interno.",
    renameItem: (name) => `Rinomina ${name}`,
    removeItem: (name) => `Elimina ${name}`,
    renameTip: "Rinomina (F2)",
    removeTip: "Elimina (tasto Canc)",
    removeTipMac: "Elimina (tasti fn+Elimina)",
    extractItem: (name: string) => `Estrai ${name}`,
    extractTip: "Copia in una cartella a tua scelta",
    extractFolder: "Estrai cartella",
    extractFolderTip: "Copia la cartella mostrata in una cartella a tua scelta",
    extractGameTip: "Copia l'intero gioco in una cartella a tua scelta",
    extractNotOnDisk: "Questa cartella esiste solo nelle modifiche in sospeso: applicale per estrarla.",
    pickExtract: "Scegli dove estrarre",
    extractPending: "Estrae i file così come sono ora, senza le modifiche in sospeso.",
    nameEmpty: "Scrivi un nome.",
    nameBad: "Un nome non può contenere / o \\, né essere . o ..",
    nameTaken: (name) => `${name} esiste già qui.`,
    markNew: "Nuovo",
    markReplaced: "Sostituito",
    markRenamed: "Rinominato",
    wasAt: (path) => `Prima: ${path}`,
    sizeLater: "Più di quanto si sia potuto elencare: il resto viene letto quando si applicano le modifiche",
    dropInto: (folder) => `Rilascia per aggiungere a ${folder}`,
    droppedGone: (path) => `${path}: il file trascinato non c'è più`,
    pending: (n) =>
      plural(n, { one: `${num(n)} modifica non ancora applicata`, other: `${num(n)} modifiche non ancora applicate` }),
    undo: "Annulla",
    undoTip: "Annulla l'ultima azione, con tutte le sue modifiche",
    discard: "Scarta tutto",
    discardTip: "Dimentica tutte le modifiche non ancora applicate",
    discardTitle: (n) =>
      plural(n, { one: `Scartare ${num(n)} modifica?`, other: `Scartare tutte le ${num(n)} modifiche?` }),
    discardText: "Vengono dimenticate e nel gioco non cambia nulla. Non si può annullare.",
    discardNo: "No",
    changesList: "Modifiche, in ordine",
    willNotApply: (n) =>
      plural(n, {
        one: `${num(n)} modifica non si può applicare così com'è. Annullala o scarta tutto.`,
        other: `${num(n)} modifiche non si possono applicare così come sono. Annullale o scarta tutto.`,
      }),
    opAdd: (to) => <>Aggiungi {to}</>,
    opAddFolder: (to) => <>Aggiungi la cartella {to}</>,
    opDelete: (path) => <>Elimina {path}</>,
    opRename: (from, to) => <>Rinomina {from} in {to}</>,
    opMkdir: (path) => <>Nuova cartella {path}</>,
    opFrom: (src) => <>Da {src}</>,
    whyInvalid: (path) => <>{path} non è un percorso valido del gioco.</>,
    whyMissing: (path) => <>{path} non è nel gioco.</>,
    whyExists: (path) => <>{path} esiste già.</>,
    whyFile: (path) => <>{path} è un file.</>,
    whyFolder: (path) => <>{path} è una cartella.</>,
    whyInto: (path) => <>{path} non può essere spostato dentro sé stesso.</>,
    conflictTitle: (n) =>
      plural(n, { one: `${num(n)} elemento esiste già qui`, other: `${num(n)} elementi esistono già qui` }),
    conflictIn: (folder) => <>In {folder}:</>,
    conflictText:
      "“Sostituisci” mette i nuovi al loro posto: una volta applicate le modifiche, il contenuto precedente è perso per sempre, e un file difettoso può impedire l'avvio del gioco. Una cartella aggiunta su una cartella viene unita, e i suoi file con lo stesso nome vengono sostituiti.",
    replace: "Sostituisci",
    skip: "Salta",
    cancel: "Annulla",
    modeLabel: "Come applicare",
    checking: "Controllo di come si possono applicare le modifiche",
    nothingToChange: "Niente da cambiare: il gioco resta com’è.",
    checkFailed: "Impossibile controllare le modifiche:",
    checkAgain: "Controlla di nuovo",
    inPlace: "Sul posto",
    rebuild: "Ricostruisci",
    rebuildNeeded: "Serve la ricostruzione",
    cantApply: "Non applicabile",
    changedDirectly: "Modificata direttamente",
    folderText: "Scritte direttamente nella cartella.",
    inPlaceText: "Scritte nell'immagine.",
    inPlaceNeeds: (need: string, free: string) => `Scritte nell'immagine; servono ${need} dei suoi ${free} liberi.`,
    noRoom: (need: string, free: string) => `Le modifiche richiedono ${need}, l'immagine ha ${free} liberi.`,
    alwaysRebuilt: "Le immagini ffpfsc e FPKG vengono sempre ricostruite.",
    rebuildText: "Accanto viene scritta una nuova immagine, che la sostituisce dopo la verifica.",
    rebuildNeeds: (need: string, free: string) =>
      `Accanto viene scritta una nuova immagine (servono circa ${need} liberi, ce ne sono ${free}), che la sostituisce dopo la verifica.`,
    uploadLater: (n: number) =>
      plural(n, {
        one: `${num(n)} file da questo browser viene controllato una volta caricato, quando premi “Applica”.`,
        other: `${num(n)} file da questo browser vengono controllati una volta caricati, quando premi “Applica”.`,
      }),
    decidedOnApply: (n: number) =>
      plural(n, {
        one: `Altre modifiche dipendono dal file da questo browser, quindi come applicarle si decide una volta caricato, quando premi “Applica”.`,
        other: `Altre modifiche dipendono dai ${num(n)} file da questo browser, quindi come applicarle si decide una volta caricati, quando premi “Applica”.`,
      }),
    sourceLocked: "Si stanno applicando modifiche a questo gioco. Scegli un'altra sorgente quando l'operazione termina.",
    notOpened: (path) => <>{path} non è stato aperto: si stanno applicando modifiche a questo gioco. Riaprilo quando l'operazione termina.</>,
    apply: "Applica",
    applyTip: "Applica tutte le modifiche elencate qui",
    applyTitle: (n: number) => plural(n, { one: `Applicare ${num(n)} modifica?`, other: `Applicare ${num(n)} modifiche?` }),
    applyInPlace: (name) => <>Vengono scritte direttamente in {name}. Una volta fatto, non si può annullare.</>,
    applyRebuild: (name) => (
      <>Accanto a {name} viene scritta una nuova immagine con le tue modifiche, verificata e poi messa al suo posto. Una volta sostituita l'immagine, non si può annullare.</>
    ),
    closeImage: "Se il gioco è in esecuzione da questa immagine, chiudilo prima.",
    closeFolder: "Se il gioco è in esecuzione da questa cartella, chiudilo prima.",
    applyRestores: "Una modifica precedente di questa immagine è stata interrotta: prima l'immagine viene riportata com'era.",
    uploadingFile: (i: number, n: number) => `Caricamento del file ${num(i)} di ${num(n)}`,
    uploading: (i: number, n: number, sent: string, total: string) => `Caricamento del file ${num(i)} di ${num(n)}: ${sent} di ${total}`,
    uploadLabel: "Caricamento",
    uploadCancelled: "Caricamento annullato. Nel gioco non è cambiato nulla.",
    applyRunning: "Applicazione delle modifiche. Potrai modificare di nuovo quando l'operazione termina (vedi “Coda”).",
    applyFailed: "Le modifiche non sono state applicate:",
    applyRefused: "Le modifiche non si possono applicare in questo modo:",
    interrupted: "Una modifica precedente di questa immagine è stata interrotta. L'immagine viene riportata com'era prima che le tue prossime modifiche vengano controllate o applicate.",
  },
  // JobList.tsx
  jobs: {
    jobs: "Operazioni",
    pending: (n) => `${num(n)} in corso o in coda`,
    allFinished: "tutte completate",
    stages: {
      scan: "Lettura della sorgente",
      preflight: "Controllo",
      check: "Controllo",
      plan: "Pianificazione",
      compress: "Compressione",
      write: "Scrittura",
      measure: "Misurazione dei pacchetti",
      pack: "Impacchettamento",
      verify: "Verifica",
      finalize: "Completamento",
      install: "Installazione",
    },
    steps: {
      scan: "Lettura",
      preflight: "Controllo",
      check: "Controllo",
      plan: "Pianificazione",
      compress: "Compressione",
      write: "Scrittura",
      measure: "Misurazione",
      pack: "Impacchettamento",
      verify: "Verifica",
      finalize: "Completamento",
      install: "Installazione",
    },
    stepsLabel: "Passaggi",
    stepDone: (s) => `Completato: ${s}`,
    stepFailed: (s) => `Non riuscito: ${s}`,
    stepLater: (s) => `Da fare: ${s}`,
    packingWriting: "Impacchettamento e scrittura",
    rate: { read: "lettura", compress: "compressione", pack: "impacchettamento", write: "scrittura" },
    showPath: "Mostra percorso",
    showFinder: "Mostra nel Finder",
    showExplorer: "Mostra in Esplora file",
    showFolder: "Mostra nella cartella",
    packTag: {
      lz4Pack: (level: string) => `LZ4: impacchetta · ${level}`,
      lz4Unpack: "LZ4: spacchetta",
      zstdPack: (level: string) => `ZSTD: impacchetta · ${level}`,
      level: (n: number) => `livello ${num(n)}`,
      zstdUnpack: "ZSTD: spacchetta",
      withRules: (tag) => `${tag} · regole personalizzate`,
    },
    edit: "Modifica",
    editRebuild: "Modifica · ricostruzione",
    transfer: "Trasferimento",
    extract: "Estrazione",
    job: (id) => `Operazione ${id}`,
    done: "Completata",
    cancelled: "Annullata",
    failed: "Non riuscita",
    cancelling: "Annullamento…",
    queued: "In coda",
    dismiss: "Ignora",
    retry: "Riprova",
    retrying: "Avvio in corso",
    stopJob: "Interrompere?",
    cancel: "Annulla",
    stopHint: "Premi di nuovo “Interrompere?” entro 4 secondi per interromperla.",
    remoteLabel: (name, device) => `${name}, un’attività su ${device}`,
    remoteTitle: (device) => `In esecuzione su ${device}, usa i file di questo dispositivo: qui puoi solo annullarla`,
    from: "da ",
    left: (d) => `${d} al termine`,
    inStep: (time) => `${time} in questo passaggio`,
    outputPath: "Percorso di output",
    sourceDeleted: (v) => `Sorgente eliminata: ${v}`,
    sourceKept: (v) => `Sorgente mantenuta: ${v}`,
    sourcePartlyDeleted: (v) => `Sorgente eliminata in parte: ${v}`,
    checks: "Controlli",
    verifiedPlain: (n) => `Verificato: ${num(n)} ${plural(n, { one: "controllo superato", many: "controlli superati", other: "controlli superati" })}`,
    verifiedFull: (n) => `Verifica completa superata: ${num(n)} ${plural(n, CHECKS)}`,
    verifiedFast: (n) => `Verifica rapida superata: ${num(n)} ${plural(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `Dati controllati: ${checked} su ${total} in ${num(n)} ${plural(n, { one: "campione", many: "campioni", other: "campioni" })}`,
    cantBuild: "Impossibile creare questo gioco:",
    cantEdit: "Impossibile applicare le modifiche:",
    cantExtract: "Impossibile estrarre:",
    cantTransfer: "Impossibile trasferire:",
    cantBuildAs: (f) => (
      <>
        Impossibile creare questo gioco come <b>{f}</b>:
      </>
    ),
    log: (n) => `Log (${num(n)})`,
    logLabel: "Log",
  },
  // Storage text shared by Files.tsx, Packages.tsx, Edit.tsx and common.tsx (FreeSpace)
  library: {
    cantDelete: "Impossibile eliminare:",
    deleted: (path) => `Eliminato ${path}`,
    storageTip: "Spazio libero nella memoria interna della console (/data)",
    free: (free, total) => (
      <>
        Spazio libero: <b>{free}</b> su {total}
      </>
    ),
    cards: "Vista a schede",
    list: "Vista a elenco",
    remove: (n) => `Elimina ${n}`,
    deleting: (n) => `Eliminazione di ${n}`,
    showAs: "Mostra come",
  },
  // Storage.tsx, Files.tsx: the Storage tab (File explorer | Package installer)
  storage: {
    views: "Vista dell'archiviazione",
    files: "Esplora file",
    title: "Esplora file",
    roots: "Unità",
    location: "Posizione",
    folders: "Cartelle",
    up: "Su",
    upTip: "Su di una cartella",
    refresh: "Aggiorna",
    filter: "Filtra per nome",
    filterLabel: "Filtra questa cartella per nome",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "cartella", other: "cartelle" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "corrispondenza", other: "corrispondenze" })}`,
    firstOf: (max, n) => plural(max, { one: `primo di ${num(n)}`, many: `primi ${num(max)} di ${num(n)}`, other: `primi ${num(max)} di ${num(n)}` }),
    truncated: "Questa cartella contiene più di 10.000 elementi: sono elencati solo i primi 10.000.",
    name: "Nome",
    size: "Dimensione",
    emptyFolder: "Questa cartella è vuota.",
    noMatch: (f) => `Nessun elemento corrisponde a “${f}”.`,
    noRoots: "Nessuna unità disponibile da esplorare.",
    cantList: "Impossibile aprire questa cartella:",
    link: "Collegamento",
    linkTip: "Un collegamento: si può aprire, ma qui non si può rinominare, spostare né eliminare",
    mounted: "Montato",
    mountedTip: "Montato da ShadowMount+: chiudi il gioco e disattiva ShadowMount+ per eliminarlo, rinominarlo o spostarlo",
    mountedNote: "Le immagini contrassegnate come Montato sono in uso da ShadowMount+: chiudi il gioco e disattiva ShadowMount+ per eliminarle, rinominarle o spostarle.",
    open: (n) => `Apri ${n}`,
    more: (n) => `Altre azioni per ${n}`,
    rename: (n) => `Rinomina ${n}`,
    renameTip: "Rinomina (F2)",
    move: (n) => `Sposta ${n}`,
    moveTip: "Sposta in un'altra cartella",
    moveTitle: (n) => `Sposta ${n} in`,
    remove: (n) => `Elimina ${n}`,
    removeTip: "Elimina (tasto Canc)",
    removeTipMac: "Elimina (tasti fn+Elimina)",
    cantDelete: "Impossibile eliminare:",
    cantRename: "Impossibile rinominare:",
    cantMove: "Impossibile spostare:",
    sameFolder: "Si trova già in quella cartella.",
    deleted: (path) => `Eliminato ${path}`,
    renamed: (path) => `Rinominato in ${path}`,
    moved: (path) => `Spostato in ${path}`,
    devices: "Dispositivi",
    thisDevice: "Questo dispositivo",
    otherDevice: "“Sposta” resta su un solo dispositivo: la schermata “Trasferimento” copia tra dispositivi.",
    loadingDevices: "Caricamento dei dispositivi",
    noDevice: "Nessun dispositivo connesso",
    noDeviceHint: "Abbina una PS5 o un computer, o aggiungi un NAS, per gestirne qui i file.",
    openConnect: "Apri “Connetti”",
    dropMove: (folder) => `Rilascia per spostare in ${folder}`,
    cantCopy: "Impossibile copiare:",
    copyNoFolder: "Apri prima una cartella su quel dispositivo.",
    copyOne: (name: string, to: string) => `Copia di ${name} in ${to}. Il trasferimento è elencato in “Coda”.`,
    copyMany: (n: number, to: string) =>
      plural(n, {
        one: `Copia di ${num(n)} elemento in ${to}. Il trasferimento è elencato in “Coda”.`,
        other: `Copia di ${num(n)} elementi in ${to}. Il trasferimento è elencato in “Coda”.`,
      }),
  },
  // build/ (Build's pack settings and checks), JobList.tsx's pack tags
  pack: {
    help: "Come funziona l'impacchettamento",
    patch: "Un pacchetto di patch non si può impacchettare.",
    additionalContent: "Il contenuto aggiuntivo (senza eboot.bin) non si può impacchettare.",
    levels: { fast: "Veloce", medium: "Medio", max: "Massimo" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Consigliato. ZSTD 5: veloce, buone dimensioni." },
        medium: { text: "ZSTD 9: più piccolo, più lento." },
        max: { text: "ZSTD 19: il più piccolo; molto più lento, più memoria." },
      },
      lz4: {
        fast: { tone: "good", text: "Consigliato. LZ4 predefinito: il più veloce." },
        medium: { text: "LZ4 HC 9: più piccolo, più lento." },
        max: { text: "LZ4 HC 12: il più piccolo, il più lento." },
      },
    },
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index non è aggiornato" : state === "damaged" ? "ampr_emu.index è danneggiato" : "Manca ampr_emu.index"}: l'impacchettamento ne scrive uno nuovo.`,
    rules: {
      title: "Regole personalizzate (TOML)",
      titleWith: (name) => `Regole personalizzate (TOML): ${name}`,
      info: "Un profilo della strategia di impacchettamento nel formato di ampr_pack.py di ampr_emu e di Lazy_AMPR decide quali file vengono impacchettati e quali restano separati. Vale l'ultima regola che corrisponde; i file di sistema restano sempre separati.",
      warn: "Regole sbagliate possono lasciare un gioco che non si avvia. Tieni una copia.",
      pickTitle: "Strategia di impacchettamento (file TOML)",
      clear: "Rimuovi",
    },
  },
  // PackHelp.tsx
  packHelp: {
    title: "Come funziona l'impacchettamento",
    close: "Chiudi",
    body: () => (
      <>
        <h3>ZSTD</h3>
        <ol>
          <li>Impacchetta gli asset del gioco in volumi ZSTD, in una cartella o in un'immagine.</li>
          <li><b>Veloce</b> (ZSTD 5) è rapido; <b>Medio</b> (9) e <b>Massimo</b> (19) sono più piccoli e più lenti.</li>
          <li>Per un gioco impacchettato con ZSTD, <b>Nessuno</b> lo spacchetta: tornano i file normali.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Alcuni giochi potrebbero non avviarsi.</b> Conserva una copia.</li>
          <li>Impacchetta in volumi LZ4, in una cartella o in un'immagine: <b>Veloce</b> (LZ4), <b>Medio</b> (HC 9), <b>Massimo</b> (HC 12).</li>
          <li>Funziona con ampr-emu-ng, come i pacchetti ZSTD.</li>
          <li>Per un gioco impacchettato con LZ4, <b>Nessuno</b> lo spacchetta.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>L'unico runtime AMPR incluso in Forge. Legge i pacchetti ZSTD e LZ4; l'impacchettamento lo installa in fakelib/.</li>
          <li>Una creazione sostituisce con esso l'ampr_emu di un gioco, a meno che “Impostazioni” non mantenga ampr_emu. Un FPKG non contiene alcun runtime: lo fornisce il loader della console.</li>
          <li>Disattiva l'aggiornamento di fakelib di ShadowMount+, altrimenti sostituisce il runtime e il gioco non riesce a leggere i suoi pacchetti.</li>
        </ol>

        <h3>Strategia di impacchettamento</h3>
        <ol>
          <li><b>Impacchetta tutto tranne i file di sistema</b> (ZSTD e LZ4): ogni file tranne eboot.bin, eseguibili e moduli, sce_sys, sce_module, fakelib e i file .ini (dlc_emu.ini resta modificabile); ampr-emu-ng serve il resto, audio e video compresi.</li>
          <li>Durante l'impacchettamento, in <b>Regole personalizzate (TOML)</b> scegli un profilo nel formato di ampr_pack.py e Lazy_AMPR, su questo dispositivo o su un altro.</li>
          <li>Vale l'ultima regola che corrisponde; i file di sistema restano sempre separati. Regole sbagliate possono lasciare un gioco che non si avvia.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "Dispositivi",
    thisDevice: "Questo dispositivo",
    drives: "Unità",
    location: "Posizione",
    parent: "Cartella superiore",
    allDrives: "Tutte le unità",
    truncated: (n) => plural(n, { one: "Viene mostrata solo la prima voce.", many: `Vengono mostrate solo le prime ${num(n)} voci.`, other: `Vengono mostrate solo le prime ${num(n)} voci.` }),
    loading: "Caricamento",
    noDrives: "Nessuna unità trovata.",
    noFolders: "Nessuna cartella qui.",
    noImages: "Nessuna cartella, immagine PS5 o archivio qui.",
    noFiles: (ext) => `Qui non ci sono cartelle né file ${dotted(ext, " o ")}.`,
    chooseFiles: (n) => (n === 0 ? "Scegli file" : `Scegli ${num(n)} ${plural(n, FILES)}`),
    nothing: "Nessuna cartella o file qui.",
    cantList: "Impossibile aprire questa cartella:",
    cancel: "Annulla",
    chooseFolder: "Scegli questa cartella",
    netOnlyHere: "Archivi e FPKG: solo da questo dispositivo.",
  },
  // bookmarks.tsx: the bookmarked folders (Transfer and the File explorer)
  bookmarks: {
    label: "Preferiti",
    add: "Aggiungi questa cartella ai preferiti",
    remove: "Rimuovi dai preferiti",
    removeNamed: (name: string) => `Rimuovi ${name} dai preferiti`,
    full: "Al massimo 9 preferiti",
  },
  // DeviceTabs.tsx
  deviceTabs: {
    add: "Aggiungi un dispositivo",
    offline: "Offline",
    offlineNamed: (name: string) => `${name} · Offline`,
  },
  // common.tsx
  common: {
    folder: "Cartella",
    pickButton: "Scegli",
    pickAny: "Cartella, immagine o archivio del gioco",
    source: "Sorgente",
    emptyTitle: "Scegli una cartella, un'immagine o un archivio di gioco",
    accepted: "Sorgenti accettate",
    unknownTitle: "Titolo sconosciuto",
    unsigned: (n: number) =>
      plural(n, {
        one: `${num(n)} eseguibile non è firmato, quindi il gioco non si avvierà. La conversione del gioco lo firma.`,
        many: `${num(n)} di eseguibili non sono firmati, quindi il gioco non si avvierà. La conversione del gioco li firma.`,
        other: `${num(n)} eseguibili non sono firmati, quindi il gioco non si avvierà. La conversione del gioco li firma.`,
      }),
    patch: "Patch",
    additionalContent: "Contenuto aggiuntivo",
    reading: "Lettura della sorgente",
    unreadable: "Sorgente illeggibile",
    backport: "Backport",
    emulators: "Emulatori",
    declares: (fw) => `dichiara ${fw}`,
    fwUnknown: "firmware non ricavabile dagli eseguibili (nessun SDK leggibile)",
    emulatorList: (names) => `emulatori: ${names}`,
    otherEmulator: "Altro",
    sdk: (v) => `SDK ${v} (dichiarato)`,
    titleId: "ID titolo",
    version: "Versione",
    firmware: "Firmware",
    fwDeclared: "dichiarato",
    fwBackport: "backport",
    fwExecutables: "eseguibili",
    size: "Dimensione",
    files: (n) => `${num(n)} ${plural(n, FILES)}`,
    contentId: "ID contenuto",
    noAmpr: "eboot.bin non importa libSceAmpr",
    traceRuntime: "Vecchio runtime di tracciamento LZ4",
    noRuntime: "Nessun runtime",
    packed: (kind) => `Impacchettato (${kind})`,
    zstdKept: "pacchetti di asset ZSTD: una conversione li mantiene",
    damaged: "Pacchetti danneggiati",
    unpacked: "Non impacchettato",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} in ${num(volumes)} ${plural(volumes, { one: "volume", many: "volumi", other: "volumi" })}` +
      (percent === null ? "" : `, ${num(percent)}% della dimensione`),
    index: {
      stale: "ampr_emu.index non aggiornato: una conversione lo rinnova",
      damaged: "ampr_emu.index danneggiato: una conversione lo riscrive",
      missing: "nessun ampr_emu.index: una conversione lo scrive",
    },
    runtimeAction: (a: "install" | "replace" | "keep" | "keepIfPacks") =>
      a === "install"
        ? "una conversione installa l'ampr-emu-ng incluso"
        : a === "replace"
          ? "una conversione sostituisce il runtime con l'ampr-emu-ng incluso"
          : a === "keep"
            ? "una conversione lo mantiene (Impostazioni)"
            : "una conversione lo mantiene se legge i pacchetti LZ4, altrimenti lo sostituisce con ampr-emu-ng",
    backportLibs: (exclusive, n) =>
      `${exclusive ? "fakelib2/, montata da sola" : "fakelib/"}, ${num(n)} ${plural(n, { one: "libreria", many: "librerie", other: "librerie" })} di backport`,
    findings: "Risultati del controllo preliminare",
    dlcTitle: (n) => `DLC incorporati (${num(n)})`,
    dlcLabel: "DLC incorporati",
    dlcAll: (note) => `Per ciascun DLC: ${note}.`,
    dlcIn: (folder, size) => `in ${folder}/, ${size}`,
    dlcNoData: "sbloccato dall'emulatore DLC (dlc_emu.ini): solo la licenza, nessun dato proprio",
    dlcStatus: (status) => `sbloccato dall'emulatore DLC (dlc_emu.ini), stato ${status || "non impostato"}`,
    dlcMerged: "integrato nelle cartelle del gioco",
    backportFiles: "File del backport",
    emulatorsIn: "Emulatori in fakelib",
    qr: (url) => `Codice QR: ${url}`,
    needsPassword: "Questo archivio richiede una password.",
    enterPassword: "Inserisci password",
    inArchive: (ext, size) => `in un archivio ${ext} · ${size}`,
    archive: "Archivio",
    parts: (n) => `${num(n)} ${plural(n, { one: "parte", many: "parti", other: "parti" })}`,
    archiveRoot: (root) => `gioco in ${root}`,
    archiveAtRoot: "gioco nella radice dell'archivio",
    solid: "solido",
    encrypted: "protetto da password",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "Eliminare questa cartella?" : "Eliminare questa immagine?"),
    text: (folder) =>
      folder
        ? "La cartella e tutto il suo contenuto verranno eliminati definitivamente: niente Cestino, nessun annullamento. Se l'eliminazione si interrompe a metà, una parte potrebbe essere già stata rimossa."
        : "Il file immagine verrà eliminato definitivamente: niente Cestino, nessun annullamento.",
    no: "No",
    yes: "Sì, elimina",
    archiveTitle: "Eliminare questo archivio?",
    archiveText:
      "Il file dell'archivio verrà eliminato definitivamente: niente Cestino, nessun annullamento. Le altre parti di un archivio in più parti restano come sono.",
    fileTitle: "Eliminare questo file?",
    fileText: "Il file verrà eliminato definitivamente: niente Cestino, nessun annullamento.",
  },
  // replace.tsx
  replace: {
    title: (n) =>
      plural(n, { one: `${num(n)} elemento esiste già lì`, other: `${num(n)} elementi esistono già lì` }),
    inFolder: (folder) => <>In {folder}:</>,
    more: (n) => `e altri ${num(n)}`,
    text: "“Sostituisci” mette i nuovi al loro posto: i vecchi file sono persi per sempre, senza Cestino e senza annullamento. Una cartella su una cartella con lo stesso nome viene unita: i file con lo stesso nome vengono sostituiti, tutto il resto lì rimane.",
    cancel: "Annulla",
    replace: "Sostituisci",
  },
  // password.tsx
  password: {
    title: "Questo archivio richiede una password",
    text: "Inserisci la sua password per aprirlo.",
    wrong: "La password non è corretta. Controllala e riprova.",
    label: "Password",
    cancel: "Annulla",
    unlock: "Sblocca",
    checking: "Verifica",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "Impossibile raggiungere il caricatore di payload sulla porta 9021. Esegui prima il tuo autoloader (jailbreak), poi apri di nuovo Dump Forge.",
      missing:
        "Nessuna copia salvata di Dump Forge trovata (né in /data/dumpforge/ né su un'unità USB). Caricalo una volta da un PC: salverà una copia per questa icona.",
      timeout:
        "Dump Forge è stato avviato ma non ha risposto entro 30 secondi. Potrebbe essere ancora in fase di avvio (una console che esce dalla modalità di riposo è lenta).",
      stopping: "Il Dump Forge precedente si sta ancora arrestando (annulla le operazioni e fa pulizia). Attendi un momento, poi riprova.",
    },
    loadFailed: (e) => `Impossibile caricare la pagina: ${e}`,
    noAnswer: "Dump Forge non ha risposto",
    cantStart: "Impossibile avviare Dump Forge",
    retry: "Riprova",
    starting: "Avvio di Dump Forge…",
    client: {
      unreachable:
        "Impossibile raggiungere Dump Forge. Verifica che il payload sia ancora in esecuzione e che questo dispositivo sia sulla stessa rete.",
      noPlatform: "Impossibile stabilire se Dump Forge è in esecuzione su una PS5. Ricarica la pagina per riprovare.",
      quitUnreachable: "Impossibile raggiungere Dump Forge per arrestarlo.",
      stillAnswering: "Dump Forge risponde ancora dopo 2 minuti; controlla la PS5.",
      gone: "Dump Forge non la segnala più (è uscita dalla cronologia delle operazioni prima che se ne vedesse il risultato)",
      uploadIncomplete: "Il caricamento non è stato completato.",
    },
  },
  // target.tsx
  target: {
    kraken: {
      fast: { label: "Veloce", note: { tone: "good", text: "Consigliato. La creazione più rapida." } },
      balanced: {
        label: "Bilanciato",
        note: {
          text: "Circa il 2,6% più piccolo di “Veloce”, ma la compressione richiede circa 6 volte il tempo, con tutti i core occupati per tutto il tempo.",
        },
      },
      smallest: {
        label: "Minimo",
        note: {
          text: "Circa il 2,7% più piccolo di “Veloce”, ma la compressione richiede circa 9 volte il tempo, con tutti i core occupati per tutto il tempo.",
        },
      },
      stored: {
        label: "Senza compressione",
        note: {
          text: "Il pacchetto è grande più o meno quanto il gioco. Provalo se un pacchetto compresso si installa ma non si avvia (segnalato con firmware 8.60 e 10.40).",
        },
      },
    },
    imageInside: "Immagine interna",
    imageInsideLabel: "Immagine all'interno del ffpfsc",
    compressionLevel: "Livello di compressione",
    compression: "Compressione",
    fullVerify: "Verifica completa",
    fullVerifyHint: "Rilegge ogni byte per controllare l'output. Richiede più tempo.",
    deleteSource: "Elimina la sorgente",
    deleteSourceHint: "Non si può annullare. La sorgente viene eliminata solo dopo una conversione riuscita.",
    deleteSourceNas: "Un gioco su un NAS non si può eliminare da qui.",
    separator: "Un nome non può contenere un separatore di cartella; usa “Cambia” per la cartella.",
    badName: "Un nome non può essere vuoto, iniziare con un punto, finire con un punto o uno spazio, né contenere caratteri di controllo.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Troppo lungo: ${bytes} byte, al massimo ${max} (UTF-8).`
        : `Troppo lungo: ${bytes} byte prima dell'estensione, al massimo ${max} (UTF-8).`,
    pkgExt: (ext) => `Termina con .${ext}, ma FPKG viene salvato come .pkg: l'estensione determina il driver.`,
    otherExt: (ext, target) =>
      `Termina con .${ext}, ma la destinazione è ${target}: l'estensione determina il driver.`,
    extractInto: "Estrai in questa cartella",
    saveInto: "Salva in questa cartella",
    output: "Output",
    change: "Cambia",
    naming: "Generazione del nome…",
    chooseSourceFirst: "Scegli prima una sorgente",
    fileName: "Nome del file",
    savedAs: (name, dir) => `salvato come ${name} in ${dir}`,
    inDir: (dir) => `in ${dir}`,
    generate: "Genera il nome in base al contenuto",
  },
  // Formats.tsx (About's Output names) and target.tsx's name pattern: generated output names
  naming: {
    title: "Nomi di output",
    parts: { title: "Titolo", title_id: "ID titolo", version: "Versione", pack: "Tipo di pacchetto", region: "Regione", fw: "Firmware" },
    template: "Modello del nome",
    partsLabel: "Inserisci una parte",
    insert: (part: string) => `Inserisci ${part}`,
    inUse: (part) => `${part} è già nel modello`,
    example: "Esempio",
    length: "Lunghezza",
    bytes: (n, max) => `${n} di ${max} byte.`,
    room: (n) => plural(n, { one: `Resta ${num(n)} byte per il titolo.`, other: `Restano ${num(n)} byte per il titolo.` }),
    noRoom: "Non resta spazio per il titolo: viene omesso.",
    versionOut: "La versione viene omessa per fare spazio.",
    cut: "Il nome viene tagliato a 63 byte.",
    idOnly: "Il testo del modello occupa i 63 byte, quindi l'ID titolo dà il nome all'output.",
    errors: {
      long: (max: number) => `Un modello è al massimo di ${max} byte.`,
      open: "Una { non ha la } di chiusura.",
      close: "Una } non ha la { di apertura.",
      unknown: (token: string) => `{${token}} non è una parte: usa Titolo, ID titolo, Versione, Tipo di pacchetto, Regione o Firmware.`,
      twice: (token) => `{${token}} compare due volte nel modello: usa ogni parte una sola volta.`,
      char: (c: string) => `Un nome non può contenere ${c}.`,
      control: "Un nome non può contenere un carattere di controllo.",
      needsName: "Un nome ha bisogno di Titolo o ID titolo.",
    },
    notSaved: "Non salvato: i nomi continuano a usare l'ultimo modello valido.",
    reset: "Ripristina",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} è disponibile`,
    update: "Aggiorna",
    later: "Più tardi",
    updating: "Aggiornamento…",
    updatingNote: "La nuova versione è in download. Questa pagina si ricaricherà da sola con la nuova versione.",
    downloading: "Il download si apre nel browser: sostituisci questa copia con il file scaricato (su Mac, trascina l'app dal .dmg in Applicazioni).",
    downloadingCli: "Il download si apre nel browser: sostituisci la CLI di ps5-dump-forge con il file scaricato oppure esegui ps5-dump-forge update --install sul computer su cui è in esecuzione.",
    failed: (v) => `Impossibile aggiornare alla v${v}`,
    timeout: (v) => `La v${v} non si è avviata entro un minuto. Se è stata scaricata, ferma Dump Forge e riaprilo per completare l’aggiornamento.`,
    retry: "Riprova",
    close: "Chiudi",
  },
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "Questo dispositivo",
    howItWorks: "Come connettersi",
    ipAddress: "Indirizzo IP",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? plural(n, { one: `visto: ${num(n)} minuto fa`, many: `visto: ${num(n)} minuti fa`, other: `visto: ${num(n)} minuti fa` })
        : unit === "h"
          ? plural(n, { one: `visto: ${num(n)} ora fa`, many: `visto: ${num(n)} ore fa`, other: `visto: ${num(n)} ore fa` })
          : unit === "d"
            ? plural(n, { one: `visto: ${num(n)} giorno fa`, many: `visto: ${num(n)} giorni fa`, other: `visto: ${num(n)} giorni fa` })
            : "visto: proprio ora",
    listenError: (e) => `Gli altri dispositivi non riescono a raggiungere questo: ${e}`,
    unavailable: (e) => `Connetti non è disponibile: ${e}`,
    loading: "Caricamento",
    discover: "Rilevamento automatico",
    stop: "Interrompi",
    fileTransfer: "Trasferimento file",
    devices: "Dispositivi",
    show: "Mostra",
    all: "Tutti",
    onlineOf: (n, total) => `${num(n)} di ${num(total)} online`,
    sendFilesTo: (name) => `Invia file a ${name}`,
    userIs: (user) => `utente ${user}`,
    nearby: "Nelle vicinanze",
    nearbyNone: "Nessun nuovo dispositivo trovato",
    addDevice: "Aggiungi un dispositivo",
    addressExample: "192.168.1.20 o nas.local",
    addByProtocol: "Oppure aggiungi per protocollo:",
    savedEmpty: "Ancora nessun dispositivo salvato",
    savedEmptyHint:
      "Il rilevamento automatico trova Forge e l’archiviazione di rete in questa rete. Abbina un dispositivo e consentilo lì, o salva un NAS.",
    online: "Online",
    offline: "Offline",
    computer: "Computer",
    address: "Indirizzo",
    keyId: "ID chiave",
    lastSeen: "Ultima volta online",
    status: "Stato",
    never: "Mai",
    unknown: "Sconosciuto",
    forget: "Dimentica",
    forgetNote: "“Dimentica” rimuove l’abbinamento solo su questo dispositivo. Per usarlo di nuovo, abbinalo ancora.",
    close: "Chiudi",
    cancel: "Annulla",
    pairTitle: "Abbina un dispositivo",
    pair: "Abbina",
    connectTo: "Connetti",
    probing: (address) => `Verifica di ${address} in corso`,
    pairing: "Abbinamento in corso",
    waitingFor: (address) => `In attesa che ${address} consenta l’abbinamento`,
    checkNumber: "Numero di controllo",
    checkHint: "Deve corrispondere al numero mostrato dall’altro dispositivo.",
    denied: "L’altro dispositivo ha rifiutato l’abbinamento.",
    timedOut: "Nessuno ha consentito l’abbinamento sull’altro dispositivo in tempo.",
    pairBusy: "L’altro dispositivo ha altre richieste di abbinamento in attesa. Riprova tra un minuto.",
    alreadyPaired: "Quel dispositivo ha già salvato questo. Dimenticalo prima lì, poi abbinalo di nuovo.",
    tooManyAttempts: "Troppe richieste rifiutate. Riprova tra 10 minuti.",
    pairPromptTitle: (name) => `Abbinare a ${name}?`,
    pairPromptBody: (name, platform, ip) =>
      `${name} (${platform}, ${ip}) vuole abbinarsi a questo dispositivo. Un dispositivo abbinato può leggere i file di questo dispositivo e aggiungerne di nuovi.`,
    pairPromptCheck: (name) => `Consentilo solo se ${name} mostra lo stesso numero di controllo.`,
    allow: "Consenti",
    deny: "Rifiuta",
    notPaired: "L’altro dispositivo non riconosce più questo. Abbinalo di nuovo.",
    searching: "Ricerca in questa rete",
    forge: "Forge",
    save: "Salva",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `Aggiungi un NAS ${protocol}`,
    nasEditTitle: (protocol) => `Modifica NAS ${protocol}`,
    nasName: "Nome",
    nasAddressHint: "Indirizzo IP o nome host; aggiungi :porta se non è 445",
    nasUser: "Nome utente",
    nasUserHint: "Lascia vuoto per accedere come ospite",
    ftpAddressHint: "Indirizzo IP o nome host; aggiungi :porta se non è 21",
    nfsAddressHint: "Indirizzo IP o nome host; aggiungi :porta se non è 2049",
    ftpUserHint: "Lascia vuoto per accedere in modo anonimo",
    ftpNote: "FTP non è cifrato, accesso compreso.",
    nfsNote: "NFS non ha un accesso: Forge si collega come root (NFSv4.1, altrimenti NFSv3), quindi l’export del server deve consentire questo dispositivo. NFS non è cifrato.",
    nasPassword: "Password",
    nasPasswordKeep: "Lascia vuoto per mantenere la password salvata",
    guest: "Ospite",
    passwordSaved: "Salvata",
    passwordNone: "Nessuna",
    edit: "Modifica",
    nasForgetNote: "“Dimentica” rimuove questo NAS e la sua password salvata da questo dispositivo.",
    thisBrowser: "Questo browser",
    uploadPick: "Scegli i file da caricare",
    uploading: (name, i, n) => `Caricamento di ${name} (${num(i)} di ${num(n)})`,
    uploaded: "Caricamento completato",
    uploadSkipped: (names) => `Non caricato, lì c'è già una cartella o un link con quel nome: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `È stato caricato ${num(done)} file su ${num(n)}: resta su questo dispositivo.`,
        other: `Sono stati caricati ${num(done)} file su ${num(n)}: restano su questo dispositivo.`,
      }),
    cantUpload: "Impossibile caricare:",
    send: "Invia",
    downloadHere: "Scarica da questo dispositivo",
    wholeRoot: "Non si può inviare un intero dispositivo, condivisione o unità: scegli una cartella al suo interno.",
    helpPhone: "Scansiona il codice QR per aprire questa pagina su un telefono. Lì, “Trasferimento” carica i file del telefono su questo dispositivo e scarica sul telefono i file di questo dispositivo.",
    help: [
      "Connetti collega Dump Forge sulla tua PS5 e sui tuoi computer nella stessa rete.",
      "Per abbinare, avvia il “Rilevamento automatico” e scegli “Abbina” in “Nelle vicinanze”, oppure scrivi un indirizzo in “Aggiungi un dispositivo”, poi consentilo sull’altro dispositivo. Consentilo solo se entrambi gli schermi mostrano lo stesso numero di controllo.",
      "Un NAS (SMB, FTP, NFS) si aggiunge allo stesso modo. Dispositivi abbinati e NAS sono sorgenti di giochi e destinazioni; i loro giochi non vengono modificati lì.",
      "“Trasferimento”, nel menu laterale, copia file e cartelle tra uno qualsiasi di essi. Un nome che esiste già lì viene sostituito solo dopo la tua conferma.",
    ],
    helpSecurity: "Niente di ciò che viene inviato tra i dispositivi è cifrato: chiunque sulla tua rete può leggerlo. La password salvata di un NAS resta su questo dispositivo non cifrata. Usa Connetti solo sulla rete di casa.",
    helpSpeed: "“Test di velocità”, nella riga di un dispositivo abbinato o nei suoi dettagli, gli invia 128 MiB e ne riceve 128 MiB, senza scrivere nulla su disco, e mostra la velocità della rete in ciascuna direzione.",
    speedTest: "Test di velocità",
    speedTestWith: (name) => `Test di velocità con ${name}`,
    speedTesting: "Test in corso",
    speedResult: (up, down) => `Invio ${up} MB/s · Ricezione ${down} MB/s`,
    speedFailed: "Test di velocità non riuscito:",
    speedNote: "Invia 128 MiB e ne riceve 128 MiB; non viene scritto nulla su disco.",
    installTitle: "Dump Forge sulla PS5",
    ps5Address: "Indirizzo della PS5",
    install: "Installa sulla PS5",
    installHint: "Richiede il loader ELF della console raggiungibile in rete (porta 9021).",
    findingRelease: "Ricerca della versione su GitHub",
    installing: (v, host) => `Installazione di Dump Forge ${v} su ${host}`,
    installed: (v, host) => `Dump Forge ${v} si sta avviando su ${host}. Associalo da Aggiungi un dispositivo quando è in esecuzione.`,
    noRelease: "Impossibile trovare la versione PS5 di Dump Forge su GitHub. Controlla la connessione a internet di questo computer e riprova.",
    otherPayload: "Qualsiasi altro payload ELF:",
    sendPayload: "Invia un payload",
    pickPayload: "Scegli un payload ELF",
    sendingPayload: (name, host) => `Invio di ${name} a ${host}`,
    sentPayload: (name, size, host) => `${name} (${size}) inviato a ${host}.`,
    helpInstallTitle: "Installa Dump Forge su una PS5 con un clic",
    helpInstallText: "Usa la scheda per installare Dump Forge e inviare il payload alla console. Scarica da GitHub la versione corrispondente (o la più recente) e la avvia.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "Installazione pacchetti",
    packages: "Pacchetti",
    refresh: "Aggiorna",
    scanning: "Ricerca dei pacchetti",
    none: "Nessun pacchetto trovato su questa console.",
    noneHint: "Copia un file .pkg nella memoria della console o su un’unità USB, poi premi Aggiorna.",
    truncated: "L’elenco potrebbe essere incompleto",
    limits: "La ricerca si è fermata a uno dei suoi limiti: potrebbero mancare alcuni pacchetti.",
    cantScan: "Impossibile cercare:",
    cantInstall: "Impossibile installare:",
    content: { game: "Gioco", patch: "Patch", additional_content: "Contenuto aggiuntivo", unknown: "Sconosciuto" },
    install: "Installa",
    installName: (name) => `Installa ${name}`,
    started: (name: string) => `Installazione di ${name} in corso. L’operazione è elencata in “Coda”.`,
    tag: "Installazione",
    title: "Installare questo pacchetto?",
    titleOn: (device) => `Installare questo pacchetto su ${device}?`,
    file: "File",
    game: "Gioco",
    titleId: "ID titolo",
    version: "Versione",
    size: "Dimensione",
    text: "La PS5 scarica il pacchetto da Dump Forge e lo installa. Tieni Dump Forge in esecuzione finché l’operazione non termina.",
    reinstall: "Se questo gioco è già installato, la PS5 elimina la copia installata prima di installare questa.",
    cancel: "Annulla",
    confirm: "Installa",
    installPkg: "Installa pkg",
    installPkgOn: (device) => `Installa un pacchetto su ${device}`,
    pickTitle: "Scegli un pacchetto da installare",
    sent: (device: string) => `${device} sta installando il pacchetto. L’operazione è elencata in “Coda”.`,
  },
  // Build's and Edit's Restore metadata online switch (build/options.tsx, Edit.tsx)
  metadata: {
    tag: "Metadati",
    cant: "Impossibile ripristinare i metadati:",
    label: "Ripristina i metadati online",
    buildHint: "Prima della compilazione scarica dal server degli aggiornamenti di Sony param.json, immagini, suono e tabelle PlayGo di questa versione e li mette nel risultato dove sono diversi. Il gioco non viene modificato. Serve internet; senza, la compilazione prosegue così com’è.",
    buildSummary: "Mette i metadati di Sony nel risultato (il gioco non viene modificato)",
    editHint: "Applica a questo gioco, con le tue modifiche, param.json, immagini, suono e tabelle PlayGo di questa versione dal server degli aggiornamenti di Sony dove sono diversi: una cartella cambia direttamente, un exfat o ffpkg sul posto, un ffpfs, ffpfsc o FPKG viene ricostruito. Serve internet; se il download non riesce, non cambia nulla.",
    editOnly: "I metadati di Sony di questa versione, applicati a questo gioco",
    editAlso: "Vengono applicati anche i metadati di Sony di questa versione, scaricati prima.",
    editConfirm: "Applicare i metadati di Sony?",
    editConfirmText: "I metadati vengono scaricati prima; se non riesce, non cambia nulla.",
  },
  // App.tsx: the side rail
  rail: {
    build: "Crea",
    settings: "Impostazioni",
    queue: "Coda",
    transfer: "Trasferimento",
    collapse: "Comprimi il menu",
    expand: "Espandi il menu",
  },
  // Queue.tsx
  queue: {
    empty: "Ancora nessuna operazione. Creazioni, modifiche, trasferimenti e installazioni compaiono qui.",
    counts: (running: number, queued: number, failed: number, done: number): string =>
      [
        running && `${num(running)} in corso`,
        queued && `${num(queued)} in coda`,
        failed && plural(failed, { one: `${num(failed)} non riuscita`, many: `${num(failed)} non riuscite`, other: `${num(failed)} non riuscite` }),
        done && plural(done, { one: `${num(done)} completata`, many: `${num(done)} completate`, other: `${num(done)} completate` }),
      ]
        .filter(Boolean)
        .join(", ") || "Niente da mostrare",
    retryFailed: (n: number): string => (n > 0 ? `Riprova le non riuscite (${num(n)})` : "Riprova le non riuscite"),
    retrying: (i: number, n: number) => `Nuovo tentativo ${num(i)} di ${num(n)}`,
    clearFinished: "Rimuovi le completate",
    filters: { all: "Tutte", running: "In corso", failed: "Non riuscite", done: "Completate" },
    filterLabel: "Mostra",
    noneShown: "Nessuna operazione corrisponde a questo filtro.",
  },
  // Settings.tsx
  settings: {
    appearance: "Aspetto",
    theme: "Tema",
    themeHint: "Lo scuro è adatto a una TV, il chiaro a una stanza luminosa.",
    dark: "Scuro",
    light: "Chiaro",
    scheme: "Schema di colori",
    schemeHint: "Il colore d’accento in tutta l’app.",
    schemes: { abyss: "Abisso", ember: "Brace", moss: "Muschio", cobalt: "Cobalto", dusk: "Crepuscolo", rose: "Rosa", sand: "Sabbia", volt: "Volt", mono: "Mono" },
    advanced: "Opzioni avanzate",
    advancedOpen: "Mostra sempre le opzioni avanzate",
    advancedHint:
      "Mostra tutti i pacchetti e i formati, senza “Altro”, e tiene aperte tutte le sezioni. Disattivalo per mantenere breve Crea.",
    bundled: (version: string) => `${version} (incluso)`,
    replaceAmprEmu: "Sostituisci ampr_emu con ampr-emu-ng",
    languageHint: "Le lingue del sistema, finché non ne scegli una.",
    notificationsGroup: "PS5",
  },
  // JobList.tsx: a finished job's result
  result: {
    file: "Output",
    type: "Tipo",
    size: "Dimensione",
    files: "File",
    time: "Tempo",
    verified: "Verificato",
    verifiedFast: "Verificato (rapido)",
    verifiedFull: "Verificato (completo)",
    notVerified: "Nessun controllo segnalato",
    checks: "Controlli:",
    copy: "Copia i dettagli",
    copied: "Copiato",
    copyFailed: "Impossibile copiare qui",
    ofSource: (pct: number) => `${num(pct, 1)}% dei file del gioco`,
    filesN: (n: number) => `${num(n)} ${plural(n, FILES)}`,
    to: "scritto in",
    took: (d: string) => `in ${d}`,
    kinds: {
      pkg: "FPKG: si installa su una PS5 con supporto FPKG",
      exfat: "immagine exfat: ShadowMount+ la monta, scrivibile",
      ffpkg: "immagine ffpkg (UFS2): ShadowMount+ la monta, scrivibile",
      ffpfs: "immagine ffpfs (PFS): ShadowMount+ la monta",
      ffpfsc: "immagine ffpfsc (PFS compresso): ShadowMount+ la monta in sola lettura",
      folder: "Cartella del gioco",
    } as Record<string, string | undefined>,
    zstdFolder: "Cartella impacchettata ZSTD",
    lz4Folder: "Cartella impacchettata LZ4",
    withPacks: (image: string, kind: string) => `${image}, file in pacchetti ${kind}`,
    edited: (what: string) => `${what}, modificato sul posto`,
    rebuilt: (what: string) => `${what}, ricostruito`,
    extracted: "File estratti",
    transferred: "File copiati",
    installed: "Pacchetto passato al programma di installazione della PS5",
  },
  // JobList.tsx: the log panel
  log: {
    lines: (n: number, of?: number): string =>
      of === undefined
        ? plural(n, { one: `${num(n)} riga`, many: `${num(n)} righe`, other: `${num(n)} righe` })
        : plural(of, { one: `${num(n)} di ${num(of)} riga`, many: `${num(n)} di ${num(of)} righe`, other: `${num(n)} di ${num(of)} righe` }),
    filter: "Filtra",
    follow: "Scorrimento automatico",
    copy: "Copia",
    copied: "Copiato",
    copyFailed: "Copia non riuscita",
    save: "Salva",
    clear: "Cancella",
    cleared: "Cancellato",
  },
  // common.tsx: the source card's fact chips
  sourceChips: {
    sdk: (v: string) => `SDK ${v}`,
    firmware: (v: string) => `FW ${v}+`,
  },
  // prefs.tsx: Advanced's badge
  advanced: {
    changed: "Modificate",
    defaults: "Predefinite",
  },
  // build/*: the Build screen
  build: {
    summary: "Riepilogo",
    addToQueue: "Aggiungi alla coda",
    queued: "Aggiunta alla coda",
    openQueue: "Apri la coda",
    space: (free: string, dir: string, needed: string) => (
      <>
        {free} liberi in <span className="path">{dir}</span>, ne servono circa {needed}
      </>
    ),
    needed: (needed: string) => `Ne servono circa ${needed}`,
    spaceRaw: "Lo spazio viene verificato come se nulla fosse compresso.",
    estimate: "stima",
    advanced: "Opzioni avanzate",
    options: "Opzioni",
    fastVerify: "Verifica rapida: ogni struttura viene controllata e un campione dei dati viene riletto.",
    fullVerify: "Verifica completa: ogni byte viene riletto.",
    deleted: "La sorgente viene eliminata dopo una creazione riuscita. Non si può annullare.",
    zstdBackport:
      "Gli asset di questo gioco sono impacchettati in ZSTD e il suo indice ZSTD include il backport: crealo prima una volta con Pacchetti: Nessuno, poi rimuovi il backport.",
    lz4Backport:
      "Gli asset di questo gioco sono impacchettati in LZ4 e il suo manifest LZ4 include il backport: crealo prima una volta con Pacchetti: Nessuno, poi rimuovi il backport.",
    backportLeftOut: (n: number) =>
      plural(n, {
        one: `${num(n)} libreria di backport è esclusa da fakelib/.`,
        many: `${num(n)} librerie di backport sono escluse da fakelib/.`,
        other: `${num(n)} librerie di backport sono escluse da fakelib/.`,
      }),
    packsTitle: "Destinazione",
    packs: "Pacchetti",
    more: "Altro",
    moreTip: "Altri formati",
    cards: {
      folder: "File normali, da copiare ovunque, ShadowMount+ lo avvia",
      pkg: "Si installa sulla PS5 come un pacchetto di gioco",
      exfat: "Si monta con ShadowMount+ e si apre su un PC",
      ffpkg: "Immagine UFS2 per ShadowMount+, montaggio scrivibile possibile",
      ffpfs: "Immagine PFS, non compressa, solo nomi di file ASCII",
      ffpfsc: "L’immagine più piccola: compressa, montata sempre in sola lettura",
      lz4: "Asset impacchettati con LZ4, cartella normale",
    },
    zstdFolderCard: "Asset impacchettati con ZSTD, cartella normale",
    packCards: {
      nonePlain: "I file del gioco così come sono",
      unpack: "Riscrive i pacchetti come file normali",
      keepZstd: "I pacchetti ZSTD vengono mantenuti byte per byte",
      keepLz4: "I pacchetti LZ4 vengono mantenuti byte per byte",
      zstd: "Gioco più piccolo, caricamento più rapido, con ampr-emu-ng",
      repackZstd: "Decodifica i pacchetti LZ4 e li reimpacchetta con ZSTD",
      lz4: "Impacchetta gli asset con LZ4, con ampr-emu-ng",
      repackLz4: "Decodifica i pacchetti ZSTD e li reimpacchetta con LZ4",
    },
    naming: "Output: il nome è in preparazione",
    output: (name: string, dir: string) => (
      <>
        Output: <b className="path">{name}</b> in <span className="path">{dir}</span>
      </>
    ),
    choices: { none: "Nessuno", zstd: "ZSTD", lz4: "LZ4" },
    levelLabel: (level: string, engine: string, n: number) => `${level}, ${engine} ${n}`,
    levelTip: (engine: string, n: number, note: string) => `${engine} ${n}. ${note}`,
    sumKeepZstd: "I pacchetti ZSTD vengono mantenuti byte per byte.",
    sumKeepLz4: "I pacchetti LZ4 vengono mantenuti byte per byte.",
    sumKeepPlain: "Nessun pacchetto viene creato: i file del gioco vengono convertiti così come sono.",
    sumUnpack: (engine: string) => `I pacchetti ${engine} vengono riscritti come file normali.`,
    sumPack: (engine: string, level: string) => `I file del gioco vengono impacchettati in volumi ${engine} (${level}).`,
    sumRepack: (from: string, to: string, level: string) => `I pacchetti ${from} vengono decodificati e reimpacchettati con ${to} (${level}).`,
    sumRules: (name: string) => `Le regole personalizzate ${name} scelgono quali file impacchettare.`,
    sumInside: (format: string, level: number) => `All’interno: ${format}, livello zlib ${level}.`,
    ngInstalled: "ampr-emu-ng viene installato in fakelib/libSceAmpr.sprx.",
    ngReplaced: (old: string) => `L’${old} del gioco viene sostituito con ampr-emu-ng.`,
    ngReplacedTrace: "Il vecchio runtime di tracciamento del gioco viene sostituito con ampr-emu-ng.",
    ngKept: (old: string) => `L’${old} del gioco viene mantenuto.`,
    ngUpdated: (v: string) => `L’ampr-emu-ng${v} del gioco viene sostituito con quello incluso.`,
    ngSame: (v: string) => `L’ampr-emu-ng${v} del gioco viene mantenuto.`,
    pkgNoRuntime: "Il runtime AMPR è escluso dall’FPKG: lo fornisce il loader della console.",
    recommended: "Consigliato",
    resultTitle: "Circa, dopo la creazione",
    resultNow: "Gioco ora",
    resultCustomRules: "regole personalizzate: nessuna stima",
    backportTitle: "Backport",
    backportNone: "Questo gioco non ha un backport.",
    backportPlain: "Solo per una conversione semplice: senza creare né scompattare pacchetti.",
  },
  // TransferPane.tsx: one device's pane of the Transfer screen
  transfer: {
    pane: (device) => `File su ${device}`,
    roots: "Unità",
    location: "Posizione",
    up: "Su",
    upTip: "Su di una cartella",
    refresh: "Aggiorna",
    filter: "Filtra per nome",
    filterLabel: "Filtra questa cartella per nome",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "cartella", many: "cartelle", other: "cartelle" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => plural(n, { one: `${num(n)} corrispondenza`, many: `${num(n)} corrispondenze`, other: `${num(n)} corrispondenze` }),
    firstOf: (max, n) => plural(max, { one: `primo di ${num(n)}`, many: `primi ${num(max)} di ${num(n)}`, other: `primi ${num(max)} di ${num(n)}` }),
    truncated: "Questa cartella contiene più di 10.000 elementi: sono elencati solo i primi 10.000.",
    name: "Nome",
    size: "Dimensione",
    select: (name) => `Seleziona ${name}`,
    open: (name) => `Apri ${name}`,
    selected: (n) =>
      plural(n, { one: `${num(n)} elemento selezionato`, many: `${num(n)} elementi selezionati`, other: `${num(n)} elementi selezionati` }),
    clear: "Annulla",
    clearTip: "Annulla la selezione",
    dragItems: (n) => plural(n, { one: `${num(n)} elemento`, many: `${num(n)} elementi`, other: `${num(n)} elementi` }),
    dropHere: (path) => `Rilascia per inviare a ${path}`,
    notHere: "Apri una cartella per inviare qui i file.",
    emptyFolder: "Questa cartella è vuota.",
    noMatch: (f) => `Nessun elemento corrisponde a “${f}”.`,
    noRoots: "Nessuna unità disponibile da esplorare.",
    cantList: "Impossibile aprire questa cartella:",
    loading: "Caricamento",
    link: "Collegamento",
    linkTip: "Un collegamento: si può aprire, ma non selezionare",
    mounted: "Montato",
    mountedTip: "Montato da ShadowMount+: chiudi il gioco e disattiva ShadowMount+ per selezionarlo",
    withTitle: "Trasferisci con",
    loadingDevices: "Caricamento dei dispositivi",
    noneTitle: "Nessun dispositivo con cui trasferire",
    noneHint: "Abbina un altro Dump Forge o aggiungi un NAS in “Connetti”, oppure accendi un dispositivo salvato.",
    openConnect: "Apri “Connetti”",
    deviceOffline: (device) => `${device} è offline. I suoi file riappariranno quando tornerà disponibile.`,
    copyTo: (n, device) =>
      plural(n, {
        one: `Copia ${num(n)} elemento su ${device}`,
        many: `Copia ${num(n)} elementi su ${device}`,
        other: `Copia ${num(n)} elementi su ${device}`,
      }),
    copyNoFolder: (device) => `Apri una cartella su ${device} in cui copiare.`,
    installOn: (device) => `Installa su ${device}`,
    dropFiles: "Rilascia qui i file per caricarli",
    uploadInto: (folder) => `Finiscono in ${folder}, la cartella mostrata su questo dispositivo.`,
    uploadNoFolder: "Apri una cartella su questo dispositivo in cui caricare.",
    foldersLeftOut: (names) => `Le cartelle non vengono caricate, solo i file: ${names}.`,
    downloadFiles: (n) => plural(n, { one: `Scarica ${num(n)} file`, many: `Scarica ${num(n)} file`, other: `Scarica ${num(n)} file` }),
    downloadNone: "Seleziona i file su questo dispositivo per scaricarli (le cartelle non vengono scaricate).",
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default it;
