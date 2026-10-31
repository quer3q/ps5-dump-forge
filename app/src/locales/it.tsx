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
    convert: "Converti",
    active: (n) => `, ${num(n)} ${plural(n, { one: "attiva", many: "attive", other: "attive" })}`,
    edit: "Modifica",
    pack: "Impacchetta",
    storage: "Archiviazione",
    connect: "Connetti",
    disconnected: "Non connesso",
    connected: (n: number) => plural(n, { one: `Connesso (${num(n)} dispositivo)`, many: `Connesso (${num(n)} dispositivi)`, other: `Connesso (${num(n)} dispositivi)` }),
    about: "Informazioni",
    prevTab: "Scheda precedente (L2)",
    nextTab: "Scheda successiva (R2)",
    offline: "Non in linea, nuovo tentativo…",
    stop: "Arresta Dump Forge",
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
  // Convert.tsx
  convert: {
    target: "Destinazione",
    targetFormat: "Formato di destinazione",
    alreadyFolder: "la sorgente è già una cartella",
    lz4Backport:
      "Gli asset di questo gioco sono impacchettati con LZ4 e il suo manifest LZ4 elenca il backport: prima spacchettalo nella scheda Impacchetta, poi rimuovi qui il backport.",
    zstdBackport:
      "Gli asset di questo gioco sono impacchettati con zstd e il suo indice zstd elenca il backport: prima spacchettalo nella scheda Impacchetta, poi rimuovi qui il backport.",
    unreadable: "Impossibile creare: non è stato possibile leggere questa sorgente (vedi “Sorgente”).",
    patchBlocked: "Impossibile creare: è un pacchetto di patch, che contiene solo ciò che l’aggiornamento ha cambiato. Aprilo in “Modifica” per estrarne i file.",
    openInEdit: "Apri in “Modifica”",
    noContentId: "Impossibile creare come fpkg: il param.json di questo gioco non ha un Content ID, che un pacchetto richiede.",
    blocked: (first, more) =>
      `Impossibile creare: ${first}${more > 0 ? ` (e ${num(more)} ${plural(more, { one: "altro", many: "altri", other: "altri" })}, vedi “Sorgente”)` : ""}.`,
    amprZstdRuntime: "Questa conversione installa ampr-emu-ng in fakelib/libSceAmpr.sprx, che serve a questi pacchetti.",
    amprNgReplaced: (version) => `Sostituisce l'ampr-emu-ng di questo gioco con l'ampr_emu ${version}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Questo gioco usa AMPR ma non ha ampr_emu: la conversione installa l'ampr_emu ${version} incluso.`;
      if (a === "trace") return `La conversione sostituisce l'ampr_emu di questo gioco con l'ampr_emu ${version} incluso.`;
      if (a === "replace")
        return own
          ? `La conversione sostituisce l'ampr_emu ${own} di questo gioco con l'ampr_emu ${version} incluso.`
          : `La conversione sostituisce l'ampr_emu di questo gioco, che non indica alcuna versione, con l'ampr_emu ${version} incluso.`;
      return own
        ? `L'ampr_emu ${own} di questo gioco è uguale o più recente dell'ampr_emu ${version} incluso: la conversione lo mantiene.`
        : `Questo gioco ha già l'ampr_emu ${version} incluso: la conversione lo mantiene.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index non elenca i file di questo gioco" : state === "damaged" ? "ampr_emu.index non è leggibile" : "Questo gioco non ha ampr_emu.index"}: la conversione ne scrive uno nuovo.`,
    amprPkgIndex: "Il pacchetto riceve un nuovo ampr_emu.index creato dai suoi file.",
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
      zstd: { tagline: "Per i giochi che usano AMPR, con ampr-emu-ng", cells: ["Caricamento fino all’80% più veloce", "Fino alla metà delle dimensioni del gioco"] },
      lz4: {
        tagline: "Sperimentale, per giochi che usano AMPR",
        cells: ["Caricamenti più rapidi del 30–40%, secondo gli autori", "Circa metà del gioco, secondo gli autori"],
      },
      ffpkg: { tagline: "Consigliato per ShadowMountPlus", cells: ["Piena velocità dell'immagine", FLAT] },
      exfat: { tagline: "Compatibilità", cells: ["Piena velocità dell'immagine", FLAT] },
      ffpfs: { tagline: "Sperimentale", cells: ["Piena velocità dell'immagine", FLAT] },
      ffpfsc: {
        tagline: "Il più piccolo, sperimentale",
        cells: ["150–250 MB/s, possibili scatti", "Dipende dal livello di compressione, di solito dal 30 al 60% in meno"],
      },
      folder: { tagline: "File del gioco così come sono", cells: ["Piena velocità dell'unità", FLAT] },
    },
    features: [
      ["Converti", "tra cartella, ffpkg, exfat, ffpfs, ffpfsc e fpkg, in entrambe le direzioni."],
      ["Trasferisci", "file e converti giochi tra la tua PS5, i computer e un NAS tramite SMB, FTP o NFS."],
      ["Modifica", "i file di un gioco come in una cartella, e controllane copertina, firmware, backport, DLC e problemi nei nomi di file prima di crearlo."],
      ["In streaming", "da qualsiasi origine a qualsiasi destinazione: senza copia temporanea, nemmeno per l’immagine dentro un ffpfsc o un gioco in un archivio."],
      ["Archivi", "uno .zip, .7z o .rar con un gioco si sfoglia, converte o impacchetta come una cartella."],
      ["USB e schede SD", "scritto su chiavette e schede formattate exFAT o FAT32."],
      ["Impacchetta", "pacchetti di asset zstd o LZ4 (sperimentale) con riparazione intelligente."],
      ["Archiviazione", "una Libreria dei tuoi giochi ed Esplora file per rinominare, spostare ed eliminare."],
      ["Sicuro", "il tuo gioco non cambia mai se non lo chiedi; i file spazzatura vengono saltati e i nomi di file errati segnalati in anticipo."],
    ],
    about: "Informazioni",
    compare: "Confronta formati",
    compareLabel: "Confronto tra formati",
    format: "Formato",
    credits: "Crediti",
    upstream: (app) =>
      `Dati degli autori: quanto riportano gli autori di ampr_emu e Lazy_AMPR, non misurato da ${app}; variano da gioco a gioco. ampr_emu 0.4.2.1 è una build di prova dei suoi autori; problema noto: alcuni giochi vanno in crash durante il salvataggio.`,
    bundled: "Le cartelle impacchettate con LZ4 funzionano con ampr_emu 0.4.2.1 di drakmor (GPL-3.0), incluso senza modifiche. Codice sorgente:",
    lazy: "L’impacchettamento LZ4 si basa su Lazy_AMPR di Nazky:",
    ng: "I pacchetti zstd e LZ4 funzionano con ampr-emu-ng. Sorgente:",
    logo: "Il logo è di Xxcco:",
    unrar:
      "Gli archivi .rar vengono letti con la libreria UnRAR inclusa, di Alexander Roshal (solo estrazione, con la licenza UnRAR).",
    language: "Lingua",
    system: "Sistema",
    notifications: "Avvisa al termine di un'attività",
    notificationsHint: "Appare sulla console per le attività di conversione, impacchettamento e trasferimento.",
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
    /** Edit's folds under the game card. */
    facts: "Dati del gioco",
    explorer: "File del gioco",
    howTitle: "Come funziona",
    /** The empty state's paragraphs. */
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
    archiveNote: "Gli archivi si possono sfogliare, ma non modificare. Per cambiarlo, prima convertilo in una cartella o in un'immagine.",
    nasNote: "I giochi su un NAS si possono sfogliare ed estrarre, ma non modificare. Per modificarlo, convertilo o estrailo prima su questo dispositivo.",
    patchNote: "È un pacchetto di patch: contiene solo ciò che l’aggiornamento ha cambiato. Sono elencati i file che contiene per intero; si possono sfogliare ed estrarre, ma non modificare.",
    peerNote: "I giochi su un altro Forge si possono sfogliare ed estrarre, ma non modificare. Per modificarlo, convertilo o estrailo prima su questo dispositivo.",
    /** The explorer's changes: none is made until Apply. */
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
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
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
    /** How the changes go in (the pending bar), from core's check before Apply. */
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
    alwaysRebuilt: "Le immagini ffpfsc e fpkg vengono sempre ricostruite.",
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
    applyRunning: "Applicazione delle modifiche. Potrai modificare di nuovo quando l'operazione qui sotto termina.",
    applyFailed: "Le modifiche non sono state applicate:",
    applyRefused: "Le modifiche non si possono applicare in questo modo:",
    applyCancelled: "L'operazione è stata annullata. Le tue modifiche sono ancora elencate qui.",
    folderPartly: "Una parte potrebbe essere già nella cartella: i file mostrati sono ciò che contiene ora.",
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
      zstdPack: (level: string) => `zstd: impacchetta · ${level}`,
      level: (n: number) => `livello ${num(n)}`,
      zstdUnpack: "zstd: spacchetta",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
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
    result: (bytes, n) => `${bytes}, ${num(n)} ${plural(n, FILES)} →`,
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
  // Library.tsx
  library: {
    pickTitle: "Scegli una cartella per la Libreria",
    scanningIn: (name) => `Scansione di ${name}`,
    scanning: "Scansione",
    cantScanFolder: "Impossibile scansionare questa cartella",
    chooseAnother: "Scegli un'altra cartella",
    pickPrompt: "Scegli la cartella della tua libreria da scansionare",
    choose: "Scegli cartella",
    cantScan: "Impossibile scansionare la Libreria:",
    cantDelete: "Impossibile eliminare:",
    deleted: (path) => `Eliminato ${path}`,
    truncated: "Limite di scansione raggiunto. L'elenco potrebbe essere incompleto.",
    limits: "La ricerca si ferma a sei livelli di cartelle e dopo circa 15 secondi di scansione.",
    noRoots: "Nessuna cartella disponibile da scansionare.",
    none: "Nessun gioco trovato. La ricerca scende fino a sei livelli di profondità e salta le cartelle di sistema. Puoi comunque scegliere altre sorgenti in “Modifica” o “Converti”.",
    skipped: "Posizioni e file saltati",
    library: "Libreria",
    storageTip: "Spazio libero nella memoria interna della console (/data)",
    free: (free, total) => (
      <>
        Spazio libero: <b>{free}</b> su {total}
      </>
    ),
    refresh: "Aggiorna",
    cards: "Vista a schede",
    list: "Vista a elenco",
    games: "Giochi",
    edit: (n) => `Modifica ${n}`,
    convert: (n) => `Converti ${n}`,
    remove: (n) => `Elimina ${n}`,
    deleting: (n) => `Eliminazione di ${n}`,
    lz4Tip: "Pacchetti di asset LZ4",
    zstdTip: "Pacchetti di asset zstd",
    showAs: "Mostra come",
    kinds: "Mostra tipo",
    all: "Tutti",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n, path) => (
      <>
        <b>{num(n)}</b> {plural(n, { one: "gioco in", many: "giochi in", other: "giochi in" })} {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "Vista dell'archiviazione",
    library: "Libreria",
    files: "Esplora file",
    title: "Esplora file",
    pickTitle: "Scegli una cartella da esplorare",
    pickPrompt: "Scegli una cartella per esplorarne i file",
    choose: "Scegli cartella",
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
    otherDevice: "“Sposta” resta su un solo dispositivo: trascinalo sulla scheda di un altro dispositivo per copiarlo lì.",
    dropCopy: (folder) => `Rilascia per copiare in ${folder}`,
    dropMove: (folder) => `Rilascia per spostare in ${folder}`,
    cantCopy: "Impossibile copiare:",
    copyNoFolder: "Apri prima una cartella su quel dispositivo.",
    copyOne: (name, to) => `Copia di ${name} in ${to}. Il trasferimento è elencato in “Connetti”.`,
    copyMany: (n, to) =>
      plural(n, {
        one: `Copia di ${num(n)} elemento in ${to}. Il trasferimento è elencato in “Connetti”.`,
        many: `Copia di ${num(n)} di elementi in ${to}. Il trasferimento è elencato in “Connetti”.`,
        other: `Copia di ${num(n)} elementi in ${to}. Il trasferimento è elencato in “Connetti”.`,
      }),
    openConnect: "Apri “Connetti”",
  },
  // Pack.tsx
  pack: {
    title: "Impacchetta",
    help: "Come funziona l'impacchettamento",
    engine: "Tipo di pacchetti",
    engines: { zstd: "zstd", lz4: "LZ4" },
    actions: { pack: "Impacchetta", unpack: "Spacchetta" },
    info: (e, packed) =>
      e === "zstd"
        ? packed
          ? "Riscrive i pacchetti zstd come file normali."
          : "Impacchetta gli asset del gioco in volumi zstd."
        : packed
          ? "Riscrive i pacchetti LZ4 come file normali."
          : "Alcuni giochi potrebbero non avviarsi. Impacchetta gli asset del gioco in volumi LZ4.",
    noAmprGame: (e) => `Questo gioco non usa AMPR, quindi i pacchetti ${e === "zstd" ? "zstd" : "LZ4"} non si possono usare.`,
    patch: "Un pacchetto di patch non si può impacchettare.",
    additionalContent: "Il contenuto aggiuntivo (senza eboot.bin) non si può impacchettare.",
    unreadable: "Impossibile avviare: non è stato possibile leggere la sorgente (vedi “Sorgente”).",
    blocked: (a, first, more) =>
      `Impossibile ${a === "pack" ? "impacchettare" : "spacchettare"}: ${first}${more > 0 ? ` (e altri ${num(more)}, vedi “Sorgente”)` : ""}.`,
    target: "Destinazione",
    packFolder: (e) => `Consigliato. Il più veloce, latenza minima: una nuova cartella di gioco, con gli asset in pacchetti ${e === "zstd" ? "zstd" : "LZ4"}.`,
    levels: { fast: "Veloce", medium: "Medio", max: "Massimo" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Consigliato. zstd 5: veloce, buone dimensioni." },
        medium: { text: "zstd 9: più piccolo, più lento." },
        max: { text: "zstd 19: il più piccolo; molto più lento, più memoria." },
      },
      lz4: {
        fast: { tone: "good", text: "Consigliato. LZ4 predefinito: il più veloce." },
        medium: { text: "LZ4 HC 9: più piccolo, più lento." },
        max: { text: "LZ4 HC 12: il più piccolo, il più lento." },
      },
    },
    runtime: "Runtime",
    runtimeNotes: {
      ng: { tone: "good", text: "Consigliato. Legge i pacchetti zstd e LZ4." },
      ampr_emu: { text: "L'ampr_emu originale." },
    },
    ngNote: "Serve ampr-emu-ng: Impacchetta lo installa in fakelib/. Disattiva l'aggiornamento di fakelib di ShadowMount+, altrimenti sostituisce il runtime.",
    installsRuntime: (version) => `Qui non c'è ampr_emu: Impacchetta installa ampr_emu ${version}.`,
    freshIndex: (state) =>
      `${state === "stale" ? "ampr_emu.index non è aggiornato" : state === "damaged" ? "ampr_emu.index è danneggiato" : "Manca ampr_emu.index"}: Impacchetta ne scrive uno nuovo.`,
    direct: "Impacchetta direttamente nell'immagine; legge il gioco due volte.",
    unpackFolder: "Una nuova cartella di gioco con file normali.",
    unpackHint: (n) =>
      n === null
        ? "Riscrive i file impacchettati come file normali."
        : `${plural(n, { one: `Riscrive ${num(n)} file impacchettato come file normale`, many: `Riscrive ${num(n)} file impacchettati come file normali`, other: `Riscrive ${num(n)} file impacchettati come file normali` })}.`,
    likelyFail: (fmt) => <>
      Probabilmente non riuscirà come <b>{fmt}</b>: non accetta i nomi di file elencati in “Sorgente”.
    </>,
    collides: (a, folder) => {
      const what = folder ? "questa cartella" : "questo file";
      return a === "pack"
        ? `Un'operazione sta già scrivendo ${what}: per impacchettare serve un altro nome.`
        : `Un'operazione sta già scrivendo ${what}: per spacchettare serve un altro nome.`;
    },
    already: (a, fmt) => a === "pack"
      ? <>Questo gioco è già in scrittura come <b>{fmt}</b>; impacchettandolo di nuovo ottieni una seconda copia.</>
      : <>Questo gioco è già in scrittura come <b>{fmt}</b>; spacchettandolo di nuovo ottieni una seconda copia.</>,
    source: "Sorgente",
  },
  // PackHelp.tsx
  packHelp: {
    title: "Come funziona l'impacchettamento",
    close: "Chiudi",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>Impacchetta gli asset del gioco in volumi zstd, in una cartella o in un'immagine. Solo per giochi che usano AMPR.</li>
          <li><b>Veloce</b> (zstd 5) è rapido; <b>Medio</b> (9) e <b>Massimo</b> (19) sono più piccoli e più lenti.</li>
          <li>Un gioco impacchettato mostra <b>Spacchetta</b>: tornano i file normali.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Alcuni giochi potrebbero non avviarsi.</b> Conserva una copia.</li>
          <li>Impacchetta in volumi LZ4, in una cartella o in un'immagine: <b>Veloce</b> (LZ4), <b>Medio</b> (HC 9), <b>Massimo</b> (HC 12).</li>
          <li>Runtime: <b>ampr-emu-ng</b> (predefinito) o l'<b>ampr_emu</b> originale.</li>
          <li>Un gioco impacchettato mostra <b>Spacchetta</b>.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>Legge i pacchetti zstd e LZ4; Impacchetta lo installa in fakelib/.</li>
          <li>Disattiva l'aggiornamento di fakelib di ShadowMount+, altrimenti sostituisce il runtime e il gioco non riesce a leggere i suoi pacchetti.</li>
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
    runtime: {
      forge_release: "Forge (versione di rilascio)",
      forge_trace: "vecchio runtime di tracciamento LZ4",
      zstd: "ampr-emu-ng",
      other: "altro",
      none: "nessuno",
    },
    packed: "Impacchettato",
    zstdKept: "pacchetti di asset zstd: una conversione li mantiene",
    damaged: "Pacchetti danneggiati",
    plain: "Non impacchettato",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} in ${num(volumes)} ${plural(volumes, { one: "volume", many: "volumi", other: "volumi" })}` +
      (percent === null ? "" : `, ${num(percent)}% della dimensione`),
    runtimeIs: (label) => `runtime: ${label}`,
    index: {
      stale: "ampr_emu.index non aggiornato: una conversione lo rinnova",
      damaged: "ampr_emu.index danneggiato: una conversione lo riscrive",
      missing: "nessun ampr_emu.index: una conversione lo scrive",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `una conversione installa l'ampr_emu ${version} incluso`
        : a === "replace"
          ? `una conversione sostituisce il runtime con l'ampr_emu ${version} incluso`
          : `ampr_emu ${version}: una conversione lo mantiene`,
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
    wrong: "La password non è corretta.",
    label: "Password",
    cancel: "Annulla",
    unlock: "Sblocca",
    checking: "Verifica…",
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
    format: {
      folder: { text: "Riporta un'immagine o un pacchetto a una normale cartella di gioco." },
      exfat: {
        text: "Per i giochi che non funzionano bene come ffpkg e girano solo come contenuto di un'unità esterna. Si apre anche su Mac o PC.",
      },
      ffpkg: {
        text: "Il file system della PS5 (UFS2). Supporta il montaggio in scrittura.",
      },
      ffpfs: {
        text: "Il file system PFS della PS5, non compresso. I nomi di file devono essere in ASCII semplice.",
      },
      ffpfsc: {
        text: "Il più piccolo: un contenitore compresso attorno a un'immagine, sempre montato in sola lettura.",
      },
      pkg: { tone: "good", text: "Consigliato. Funziona come un gioco nativo, ma solo se il firmware lo consente." },
      lz4: {
        tone: "warn",
        text: "Sperimentale. Una cartella di gioco con gli asset impacchettati in volumi LZ4, che l'emulatore AMPR incluso (ampr_emu) spacchetta mentre il gioco è in esecuzione. Solo per giochi che usano AMPR.",
      },
    },
    innerFfpkg: { text: "Il file system della PS5 (UFS2)." },
    innerExfat: { tone: "good", text: "Consigliato. L'immagine più stabile in un contenitore compresso." },
    level0: { text: "Nessuna compressione: ogni blocco viene salvato così com'è. Il file è grande quanto l'immagine." },
    levelDefault: {
      tone: "good",
      text: "Consigliato: il valore predefinito di zlib. Con il livello 1 la creazione è circa 3 volte più rapida e il file è circa il 3% più grande; con il livello 9 il file è solo leggermente più piccolo.",
    },
    levelLow: {
      text: "La creazione è più rapida rispetto al livello 6 (circa 3 volte con il livello 1); il file risulta più grande (di circa il 3% con il livello 1).",
    },
    levelHigh: {
      text: "Il file è appena più piccolo rispetto al livello 6 (dello 0,04% con il livello 9), ma la compressione richiede più tempo (circa il 25% in più con il livello 9), con tutti i core occupati per tutta la durata.",
    },
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
    pkgExt: (ext) => `Termina con .${ext}, ma fpkg viene salvato come .pkg: l'estensione determina il driver.`,
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
    generatePattern: "[nome del gioco]-[ID titolo]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} è disponibile`,
    update: "Aggiorna",
    later: "Più tardi",
    updating: "Aggiornamento…",
    updatingNote: "La nuova versione è in download. Questa pagina si ricaricherà da sola con la nuova versione.",
    downloading: "Download nel browser: estrai l'archivio e sostituisci questa copia con il suo contenuto.",
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
    details: (name) => `Dettagli di ${name}`,
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
    pickSendFiles: "File da inviare",
    pickSendFolder: "Cartella da inviare",
    pickSaveTo: "Dove salvarlo",
    thisBrowser: "Questo browser",
    browserHint: "I file di questo telefono o computer finiscono in una cartella di questo dispositivo, e un file di questo dispositivo viene salvato da questo browser.",
    uploadTo: "Cartella in cui caricare",
    uploadPick: "Scegli i file da caricare",
    uploading: (name, i, n) => `Caricamento di ${name} (${num(i)} di ${num(n)})`,
    uploaded: "Caricamento completato",
    uploadSkipped: (names) => `Non caricato, lì c'è già una cartella o un link con quel nome: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `È stato caricato ${num(done)} file su ${num(n)}: resta su questo dispositivo.`,
        other: `Sono stati caricati ${num(done)} file su ${num(n)}: restano su questo dispositivo.`,
      }),
    cantStart: "Impossibile avviare il trasferimento:",
    cantUpload: "Impossibile caricare:",
    pickDownload: "File da scaricare",
    transferIntro: "Invia file o una cartella da qualsiasi dispositivo a una cartella qui, su un dispositivo abbinato o su un NAS. Il trasferimento viene eseguito come operazione nell’elenco delle operazioni.",
    send: "Invia",
    sendWhat: "Cosa inviare",
    sendTo: "Invia a",
    sendNeedsDevice: "Collega prima un altro dispositivo: abbina un Forge o aggiungi un NAS.",
    nothingChosen: "Ancora nulla di scelto",
    chooseFiles: "Scegli file",
    chooseFolder: "Scegli una cartella",
    startTransfer: "Avvia",
    uploadHere: "Carica su questo dispositivo",
    downloadHere: "Scarica da questo dispositivo",
    consoleNoFiles: "Il browser della console non può scegliere file: apri questa pagina su un telefono o un computer.",
    wholeRoot: "Non si può inviare un intero dispositivo, condivisione o unità: scegli una cartella al suo interno.",
    helpPhone: "Scansiona il codice QR per aprire questa pagina su un telefono. Lì, “Carica su questo dispositivo” e “Scarica da questo dispositivo” in “Trasferimento file” spostano i file del telefono.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "Connetti collega Dump Forge sulla tua PS5 e sui tuoi computer nella stessa rete.",
      "Per abbinare, avvia il “Rilevamento automatico” e scegli “Abbina” in “Nelle vicinanze”, oppure scrivi un indirizzo in “Aggiungi un dispositivo”, poi consentilo sull’altro dispositivo. Consentilo solo se entrambi gli schermi mostrano lo stesso numero di controllo.",
      "Un NAS (SMB, FTP, NFS) si aggiunge allo stesso modo. Dispositivi abbinati e NAS sono sorgenti di giochi e destinazioni; i loro giochi non vengono modificati lì.",
      "“Trasferimento file” copia file e cartelle tra uno qualsiasi di essi. Un nome che esiste già lì viene sostituito solo dopo la tua conferma.",
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
    started: (name) => `Installazione di ${name} in corso. L’operazione è elencata qui sotto.`,
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
    sent: (device) => `${device} sta installando il pacchetto. L’operazione è elencata qui sotto.`,
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default it;
