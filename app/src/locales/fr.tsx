// French. Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// French typography: a narrow no-break space (U+202F) before : ; ? ! and %, and inside « ».
// Terms: trace/traces, traçage; empaqueter/dépaqueter; patcher/retirer le patch; runtime.

import { dotted, num, plural } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "Égale à la taille du jeu";
const FILES = { one: "fichier", many: "de fichiers", other: "fichiers" };
const CHECKS = { one: "contrôle", many: "de contrôles", other: "contrôles" };

const fr: Messages = {
  // App.tsx
  app: {
    address: (url) => `Adresse : ${url}. Afficher comment se connecter, avec un code QR`,
    screens: "Écrans",
    convert: "Convertir",
    active: (n) => `, ${num(n)} en cours`,
    edit: "Modifier",
    library: "Bibliothèque",
    about: "À propos",
    offline: "Hors ligne, nouvelle tentative…",
    stop: "Arrêter Dump Forge",
    stillRunning: (n) =>
      n === 1
        ? "Une tâche est encore en cours"
        : `${num(n)} ${plural(n, { one: "tâche est encore en cours", many: "de tâches sont encore en cours", other: "tâches sont encore en cours" })}`,
    job: (id) => `Tâche ${id}`,
    quitNote: (web, n) =>
      n === 1
        ? `${web ? "Arrêter Dump Forge" : "Quitter"} interrompt la tâche et supprime sa sortie inachevée.`
        : `${web ? "Arrêter Dump Forge" : "Quitter"} interrompt les tâches et supprime leurs sorties inachevées.`,
    keepRunning: "Continuer",
    stopAndQuit: (n) => (n === 1 ? "Arrêter la tâche et quitter" : "Arrêter les tâches et quitter"),
    stoppingJobs: "Arrêt des tâches et nettoyage…",
    stoppingForge: "Arrêt de Dump Forge…",
    couldntStop: "Impossible d’arrêter Dump Forge",
    close: "Fermer",
    tryAgain: "Réessayer",
    stopped: "Dump Forge est arrêté",
    startAgain: (ps5) => (ps5 ? "Pour continuer, rouvrez la vignette Dump Forge sur la PS5." : "Pour continuer, relancez la même commande ps5-dump-forge serve."),
    confirmStop: (web: boolean) => (web ? "Arrêter Dump Forge ?" : "Quitter PS5 Dump Forge ?"),
    stopNote: (web, ps5) => (!web ? "Aucune tâche en cours." : ps5 ? "Pour le relancer, ouvrez la vignette Dump Forge sur la PS5." : "Pour le relancer, exécutez de nouveau la même commande ps5-dump-forge serve."),
    quit: "Quitter",
    github: (name) => `${name} sur GitHub`,
    queued: "en attente",
    progress: (percent, left) => `${percent} % terminé${left ? `, il reste ${left}` : ""}`,
  },
  // Convert.tsx
  convert: {
    target: "Cible",
    targetFormat: "Format cible",
    alreadyFolder: "la source est déjà un dossier",
    lz4Backport:
      "Les ressources de ce jeu sont empaquetées en LZ4 et son manifeste LZ4 inclut le backport : dépaquetez-le d’abord dans l’onglet « LZ4 », puis retirez le backport ici.",
    unreadable: "Création impossible : cette source n’a pas pu être lue (voir « Source »).",
    noContentId: "Création en fpkg impossible : le param.json de ce jeu n’a pas de Content ID, indispensable à un paquet.",
    blocked: (first, more) =>
      `Création impossible : ${first}${more > 0 ? ` (et ${num(more)} de plus, voir « Source »)` : ""}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Ce jeu utilise AMPR mais n’a pas d’ampr_emu : la conversion installe l’ampr_emu ${version} fourni.`;
      if (a === "trace") return `La conversion remplace le runtime de traçage LZ4 par l’ampr_emu ${version} de production fourni.`;
      if (a === "replace")
        return own
          ? `La conversion remplace l’ampr_emu ${own} de ce jeu par l’ampr_emu ${version} fourni.`
          : `La conversion remplace l’ampr_emu de ce jeu, qui n’indique aucune version, par l’ampr_emu ${version} fourni.`;
      return own
        ? `L’ampr_emu ${own} de ce jeu est aussi récent que l’ampr_emu ${version} fourni, ou plus : la conversion le garde.`
        : `Ce jeu a déjà l’ampr_emu ${version} fourni : la conversion le garde.`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "ampr_emu.index ne liste pas les fichiers de ce jeu" : state === "damaged" ? "ampr_emu.index est illisible" : "Ce jeu n’a pas d’ampr_emu.index"} : la conversion en écrit un nouveau${traces ? " et laisse de côté les anciennes traces" : ""}.`,
    amprPkgIndex: (traces) =>
      `Le paquet reçoit un nouvel ampr_emu.index construit à partir de ses fichiers${traces ? ", sans les anciennes traces" : ""}.`,
    removeBackport: "Retirer le backport",
    removeHint: (libs, emulators, other) =>
      `Exclut de fakelib/ ${num(libs)} ${plural(libs, { one: "bibliothèque", many: "de bibliothèques", other: "bibliothèques" })} du backport` +
      (emulators.length === 0
        ? other
          ? " ; les autres bibliothèques homebrew sont conservées."
          : "."
        : ` ; ${plural(emulators.length, { one: "l’émulateur", many: "les émulateurs", other: "les émulateurs" })} (${emulators.join(", ")})${other ? " et les autres bibliothèques homebrew" : ""} ${emulators.length === 1 && !other ? "est conservé" : "sont conservés"}.`),
    cantRemove: (why) => `Impossible de retirer le backport : ${why}`,
    compare: "Comparer les formats",
    build: "Créer",
    likelyFail: (fmt) => (
      <>
        Risque d’échec en <b>{fmt}</b> : ce format refuse des noms de fichiers listés dans « Source ».
      </>
    ),
    collides: "Une tâche écrit déjà ce fichier : relancer « Créer » échouerait. Choisissez un autre nom.",
    already: (fmt) => (
      <>
        Ce jeu est déjà en cours de création en <b>{fmt}</b> ; relancer « Créer » en fait une seconde copie.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "Bon", ok: "Compromis", bad: "Inconvénient" },
    columns: ["Vitesse sur la console", "Taille sur le disque"],
    rows: {
      pkg: {
        tagline: "Le meilleur, si votre firmware le permet",
        cells: ["Native, pleine vitesse", "30–60 % de moins, selon le jeu"],
      },
      lz4: {
        tagline: "Expérimental, pour les jeux qui utilisent AMPR",
        cells: ["Chargement 30–40 % plus rapide, selon les auteurs", "Environ la moitié de la taille du jeu, selon les auteurs"],
      },
      ffpkg: { tagline: "Recommandé pour ShadowMountPlus", cells: ["Lecture de l’image à pleine vitesse", FLAT] },
      exfat: { tagline: "Compatibilité", cells: ["Lecture de l’image à pleine vitesse", FLAT] },
      ffpfs: { tagline: "Expérimental", cells: ["Lecture de l’image à pleine vitesse", FLAT] },
      ffpfsc: {
        tagline: "Le plus compact, expérimental",
        cells: ["150–250 Mo/s, saccades possibles", "Dépend du niveau de compression, en général 30–60 % de moins"],
      },
      folder: { tagline: "Fichiers du jeu tels quels", cells: ["Pleine vitesse du disque", FLAT] },
    },
    features: [
      ["Conversion", "entre un dossier, ffpkg, exfat, ffpfs, ffpfsc et fpkg, dans les deux sens."],
      ["Modification", "des fichiers d’un jeu comme dans un dossier, avec sa jaquette, son firmware, son backport, ses DLC et ses problèmes de noms de fichiers avant la création."],
      ["Suppression", "d’un jeu du disque, après confirmation."],
      ["LZ4", "traçage d’un jeu AMPR, puis empaquetage de ses ressources avec réparation intelligente (expérimental)."],
      ["En flux", "de n’importe quelle source vers n’importe quelle cible : sans copie temporaire, même pour l’image dans un ffpfsc ou un jeu dans une archive."],
      ["Archives", "un .zip, .7z ou .rar contenant un jeu se parcourt, se convertit ou s’empaquette comme un dossier."],
      ["Bibliothèque", "les jeux de cet ordinateur ou de cette console, prêts à convertir et à modifier."],
      ["Sûr", "votre jeu n’est jamais modifié sans que vous le demandiez ; les fichiers parasites sont ignorés et les mauvais noms de fichiers signalés d’avance."],
      ["USB et cartes SD", "écrit sur des clés et cartes formatées en exFAT ou FAT32."],
      ["Sur place ou reconstruit", "les modifications vont directement dans l’image si elles tiennent, sinon dans une nouvelle image qui la remplace après vérification."],
    ],
    howToConnect: "Comment se connecter",
    connectText: (url) => (
      <>Ouvrez {url} dans un navigateur sur n’importe quel appareil de votre réseau local, ou scannez ce code.</>
    ),
    close: "Fermer",
    about: "À propos",
    aboutText: "Cet outil convertit un jeu de n’importe quel format vers n’importe quel autre, sur toutes les plateformes, PS5 comprise.",
    connectAny: "Se connecter depuis n’importe quel appareil",
    useAddress: (url) => <>Utilisez {url} ou scannez ce code.</>,
    compare: "Comparer les formats",
    compareLabel: "Comparaison des formats",
    format: "Format",
    upstream: (app) =>
      `Chiffres des auteurs : ce qu’annoncent les auteurs d’ampr_emu et de Lazy_AMPR, non mesuré par ${app} ; ils varient selon le jeu. ampr_emu 0.4.2.1 est une version de test de l’auteur ; problème connu : certains jeux plantent lors de la sauvegarde.`,
    bundled: "Les dossiers empaquetés en LZ4 fonctionnent avec ampr_emu 0.4.2.1 de drakmor (GPL-3.0), fourni sans modification. Source :",
    lazy: "La méthode « tracer, puis empaqueter » reprend Lazy_AMPR de Nazky :",
    unrar:
      "Les archives .rar sont lues avec la bibliothèque UnRAR fournie, d’Alexander Roshal (extraction seule, sous la licence UnRAR).",
    language: "Langue",
    system: "Système",
  },
  // Edit.tsx
  edit: {
    details: "Détails",
    deleting: "Suppression de la source…",
    delete: "Supprimer la source",
    deleted: "Supprimé :",
    filter: "Filtrer par chemin",
    filterLabel: "Filtrer les fichiers par chemin",
    firstOf: (max, n) => `${num(max)} premiers sur ${num(n)}`,
    fileCount: (n) => `${num(n)} ${plural(n, FILES)}`,
    noMatch: (f) => `Aucun fichier ne correspond à « ${f} ».`,
    noListing: "La liste des fichiers d’une archive n’est pas prise en charge.",
    noParam: "Pas de sce_sys/param.json.",
    reader: "Lecteur",
    leftovers: "Fichiers et dossiers restants",
    found: (n) => `${num(n)} ${plural(n, { one: "trouvé", many: "trouvés", other: "trouvés" })}`,
    leftoversNote: (app) =>
      `Des sorties inachevées, généralement laissées par des tâches interrompues. Ne supprimez que celles que vous reconnaissez, et seulement quand aucune tâche n’est en cours ; ${app} ne les supprime jamais lui-même.`,
    checked: (n) => `Dossiers vérifiés (${num(n)})`,
    /** Edit's folds under the game card. */
    facts: "Infos sur le jeu",
    explorer: "Fichiers du jeu",
    howTitle: "Comment ça marche",
    /** The empty state's paragraphs. */
    how: (app) => [
      "Ouvrez le **dossier** ou **l’image** d’un jeu, puis modifiez-le comme un dossier sur une clé USB : **ajoutez et remplacez des fichiers** (glissez-les ici ou utilisez « Ajouter »), **renommez, supprimez**, créez des dossiers.",
      `Rien ne change **tant que vous n’appuyez pas sur « Appliquer »** : vos modifications s’accumulent ici, puis ${app} les écrit d’un coup directement dans l’image ou le dossier et vérifie le résultat. **Ce n’est pas un disque monté** : ${app} garde la trace de vos modifications et les applique lui-même.`,
      `Quand les modifications tiennent, elles vont **directement dans l’image** ; sinon (plus de place à l’intérieur, ou un format qui ne les accepte pas), ${app} crée à côté une nouvelle image avec vos modifications et la met à sa place une fois vérifiée. Dans les deux cas, en cas d’échec, l’image reste telle quelle.`,
      "Les archives peuvent être parcourues, mais pas modifiées.",
    ],
    location: "Emplacement dans le jeu",
    root: "Jeu",
    up: "Remonter",
    upTip: "Remonter d’un dossier",
    name: "Nom",
    size: "Taille",
    items: (n) => `${num(n)} ${plural(n, { one: "élément", many: "d’éléments", other: "éléments" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "dossier", many: "de dossiers", other: "dossiers" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "correspondance", many: "de correspondances", other: "correspondances" })}`,
    emptyFolder: "Ce dossier est vide.",
    open: (name) => `Ouvrir ${name}`,
    archiveNote: "Les archives peuvent être parcourues, mais pas modifiées. Pour la modifier, convertissez-la d’abord en dossier ou en image.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "Ajouter des fichiers",
    addFolder: "Ajouter un dossier",
    pickAddFiles: "Choisissez les fichiers à ajouter",
    pickAddFolder: "Choisissez le dossier à ajouter",
    nameLabel: "Nom",
    renameItem: (name) => `Renommer ${name}`,
    removeItem: (name) => `Supprimer ${name}`,
    renameTip: "Renommer (F2)",
    removeTip: "Supprimer (touche Suppr)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "Extraire",
    extractItem: (name: string) => `Extraire ${name}`,
    extractTip: "Copier dans un dossier de votre choix",
    extractFolder: "Extraire le dossier",
    extractFolderTip: "Copier le dossier affiché dans un dossier de votre choix",
    extractGameTip: "Copier tout le jeu dans un dossier de votre choix",
    pickExtract: "Choisissez où extraire",
    extractPending: "Extrait les fichiers tels qu’ils sont maintenant, sans vos modifications en attente",
    nameEmpty: "Saisissez un nom.",
    nameBad: "Un nom ne peut pas contenir / ni \\, ni être . ou ..",
    nameTaken: (name) => `${name} existe déjà ici.`,
    markNew: "Nouveau",
    markReplaced: "Remplacé",
    markRenamed: "Renommé",
    wasAt: (path) => `Avant : ${path}`,
    sizeLater: "Plus que ce qui a pu être listé : le reste est lu quand les modifications sont appliquées",
    dropInto: (folder) => `Déposez pour ajouter à ${folder}`,
    pending: (n) =>
      plural(n, {
        one: `${num(n)} modification pas encore appliquée`,
        many: `${num(n)} de modifications pas encore appliquées`,
        other: `${num(n)} modifications pas encore appliquées`,
      }),
    undo: "Annuler",
    undoTip: "Annuler la dernière modification",
    discard: "Tout abandonner",
    discardTip: "Oublier toutes les modifications pas encore appliquées",
    discardTitle: (n) =>
      plural(n, { one: `Abandonner ${num(n)} modification ?`, other: `Abandonner les ${num(n)} modifications ?` }),
    discardText: "Elles sont oubliées, et rien ne change dans le jeu. Impossible de revenir en arrière.",
    discardNo: "Non",
    changesList: "Modifications, dans l’ordre",
    willNotApply: (n) =>
      plural(n, {
        one: `${num(n)} modification ne peut pas s’appliquer telle quelle. Annulez-la, ou abandonnez tout.`,
        other: `${num(n)} modifications ne peuvent pas s’appliquer telles quelles. Annulez-les, ou abandonnez tout.`,
      }),
    opAdd: (to) => <>Ajouter {to}</>,
    opAddFolder: (to) => <>Ajouter le dossier {to}</>,
    opDelete: (path) => <>Supprimer {path}</>,
    opRename: (from, to) => <>Renommer {from} en {to}</>,
    opMkdir: (path) => <>Nouveau dossier {path}</>,
    opFrom: (src) => <>Depuis {src}</>,
    whyInvalid: (path) => <>{path} n’est pas un chemin valide dans le jeu.</>,
    whyMissing: (path) => <>{path} n’est pas dans le jeu.</>,
    whyExists: (path) => <>{path} existe déjà.</>,
    whyFile: (path) => <>{path} est un fichier.</>,
    whyFolder: (path) => <>{path} est un dossier.</>,
    whyInto: (path) => <>{path} ne peut pas être déplacé dans lui-même.</>,
    conflictTitle: (n) =>
      plural(n, { one: `${num(n)} élément existe déjà ici`, other: `${num(n)} éléments existent déjà ici` }),
    conflictIn: (folder) => <>Dans {folder} :</>,
    conflictText:
      "« Remplacer » met les nouveaux à leur place : une fois les modifications appliquées, l’ancien contenu est perdu pour de bon, et un fichier défectueux peut empêcher le jeu de démarrer. Un dossier ajouté sur un dossier est fusionné, ses fichiers de même nom remplacés.",
    replace: "Remplacer",
    skip: "Ignorer",
    cancel: "Annuler",
    /** How the changes go in (the pending bar), from core's check before Apply. */
    modeLabel: "Mode d’application",
    checking: "Vérification de la façon d’appliquer les modifications",
    checkFailed: "Impossible de vérifier les modifications :",
    checkAgain: "Vérifier à nouveau",
    inPlace: "Sur place",
    rebuild: "Reconstruire",
    rebuildNeeded: "Reconstruction nécessaire",
    cantApply: "Impossible d’appliquer",
    changedDirectly: "Modifié directement",
    folderText: "Écrit directement dans le dossier.",
    inPlaceText: "Écrit dans l’image.",
    inPlaceNeeds: (need: string, free: string) => `Écrit dans l’image ; il faut ${need} sur ses ${free} libres.`,
    noRoom: (need: string, free: string) => `Les modifications demandent ${need}, l’image a ${free} libres.`,
    alwaysRebuilt: "Les images ffpfsc et fpkg sont toujours reconstruites.",
    rebuildText: "Une nouvelle image est écrite à côté et la remplace une fois vérifiée.",
    rebuildNeeds: (need: string, free: string) =>
      `Une nouvelle image est écrite à côté (environ ${need} libres nécessaires, ${free} libres) et la remplace une fois vérifiée.`,
    uploadLater: (n: number) =>
      plural(n, {
        one: `${num(n)} fichier de ce navigateur est vérifié une fois envoyé, quand vous appuyez sur « Appliquer ».`,
        other: `${num(n)} fichiers de ce navigateur sont vérifiés une fois envoyés, quand vous appuyez sur « Appliquer ».`,
      }),
    decidedOnApply: (n: number) =>
      plural(n, {
        one: `D’autres modifications dépendent du fichier de ce navigateur : leur mode d’application est donc décidé une fois celui-ci envoyé, quand vous appuyez sur « Appliquer ».`,
        other: `D’autres modifications dépendent des ${num(n)} fichiers de ce navigateur : leur mode d’application est donc décidé une fois ceux-ci envoyés, quand vous appuyez sur « Appliquer ».`,
      }),
    sourceLocked: "Des modifications sont en cours d’application sur ce jeu. Choisissez une autre source une fois la tâche terminée.",
    notOpened: (path) => <>{path} n’a pas été ouvert : des modifications sont en cours d’application sur ce jeu. Rouvrez-le une fois la tâche terminée.</>,
    apply: "Appliquer",
    applyTip: "Appliquer toutes les modifications listées ici",
    applyTitle: (n: number) => plural(n, { one: `Appliquer ${num(n)} modification ?`, other: `Appliquer ${num(n)} modifications ?` }),
    applyInPlace: (name) => <>Elles sont écrites directement dans {name}. Impossible de revenir en arrière une fois terminé.</>,
    applyRebuild: (name) => (
      <>Une nouvelle image avec vos modifications est écrite à côté de {name}, vérifiée, puis mise à sa place. Impossible de revenir en arrière une fois l’image remplacée.</>
    ),
    closeImage: "Si le jeu tourne depuis cette image, fermez-le d’abord.",
    closeFolder: "Si le jeu tourne depuis ce dossier, fermez-le d’abord.",
    applyRestores: "Une édition précédente de cette image a été interrompue : l’image est d’abord remise dans son état d’origine.",
    uploading: (i: number, n: number, sent: string, total: string) => `Envoi du fichier ${num(i)} sur ${num(n)} : ${sent} sur ${total}`,
    uploadLabel: "Envoi",
    uploadCancelled: "Envoi annulé. Rien n’a changé dans le jeu.",
    applyRunning: "Application des modifications. Vous pourrez de nouveau modifier une fois la tâche ci-dessous terminée.",
    applyFailed: "Les modifications n’ont pas été appliquées :",
    applyRefused: "Les modifications ne peuvent pas être appliquées ainsi :",
    applyCancelled: "La tâche a été annulée. Vos modifications sont toujours listées ici.",
    folderPartly: "Une partie est peut-être déjà dans le dossier : les fichiers affichés sont ce qu’il contient maintenant.",
    interrupted: "Une édition précédente de cette image a été interrompue. L’image est remise dans son état d’origine avant que vos prochaines modifications soient vérifiées ou appliquées.",
  },
  // JobList.tsx
  jobs: {
    jobs: "Tâches",
    pending: (n) => `${num(n)} en cours ou en attente`,
    allFinished: "toutes terminées",
    stages: {
      scan: "Lecture de la source",
      preflight: "Contrôle",
      check: "Contrôle",
      plan: "Planification",
      compress: "Compression",
      write: "Écriture",
      measure: "Calcul des paquets",
      pack: "Empaquetage",
      verify: "Vérification",
      finalize: "Finalisation",
    },
    steps: {
      scan: "Lecture",
      preflight: "Contrôle",
      check: "Contrôle",
      plan: "Planification",
      compress: "Compression",
      write: "Écriture",
      measure: "Mesure",
      pack: "Empaquetage",
      verify: "Vérification",
      finalize: "Finalisation",
    },
    stepsLabel: "Étapes",
    stepDone: (s) => `Terminé : ${s}`,
    stepLater: (s) => `À venir : ${s}`,
    packingWriting: "Empaquetage et écriture",
    rate: { read: "lecture", compress: "compression", pack: "empaquetage", write: "écriture" },
    showPath: "Afficher le chemin",
    showFinder: "Afficher dans le Finder",
    showExplorer: "Afficher dans l’Explorateur",
    showFolder: "Afficher dans le dossier",
    lz4: {
      pack: "LZ4 empaquetage",
      unpack: "LZ4 dépaquetage",
      trace: "LZ4 traçage",
      unpatch: "LZ4 retrait du patch",
      runtime: "Runtime AMPR",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "Modification",
    editRebuild: "Modification · reconstruction",
    extract: "Extraction",
    job: (id) => `Tâche ${id}`,
    done: "Terminé",
    cancelled: "Annulé",
    failed: "Échec",
    cancelling: "Annulation…",
    queued: "En attente",
    dismiss: "Masquer",
    stopJob: "Arrêter ?",
    cancel: "Annuler",
    stopHint: "Cliquez de nouveau sur « Arrêter ? » dans les 4 secondes pour arrêter la tâche.",
    from: "depuis ",
    left: (d) => `il reste ${d}`,
    inStep: (time) => `${time} dans cette étape`,
    result: (bytes, n) => `${bytes}, ${num(n)} ${plural(n, FILES)} →`,
    outputPath: "Chemin de sortie",
    checks: "Contrôles",
    verifiedPlain: (n) => `Vérifié : ${num(n)} ${plural(n, CHECKS)} ${plural(n, { one: "réussi", many: "réussis", other: "réussis" })}`,
    verifiedFull: (n) => `Vérification complète réussie : ${num(n)} ${plural(n, CHECKS)}`,
    verifiedFast: (n) => `Vérification rapide réussie : ${num(n)} ${plural(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `Données vérifiées : ${checked} sur ${total}, en ${num(n)} ${plural(n, { one: "échantillon", many: "d’échantillons", other: "échantillons" })}`,
    cantBuild: "Impossible de créer ce jeu :",
    cantEdit: "Impossible d’appliquer les modifications :",
    cantExtract: "Impossible d’extraire :",
    cantBuildAs: (f) => (
      <>
        Impossible de créer ce jeu en <b>{f}</b> :
      </>
    ),
    log: (n) => `Journal (${num(n)})`,
    logLabel: "Journal",
  },
  // Library.tsx
  library: {
    pickTitle: "Choisir un dossier pour la bibliothèque",
    scanningIn: (name) => `Analyse de ${name}`,
    scanning: "Analyse",
    cantScanFolder: "Impossible d’analyser ce dossier",
    chooseAnother: "Choisir un autre dossier",
    pickPrompt: "Choisissez le dossier de votre bibliothèque à analyser",
    choose: "Choisir un dossier",
    cantScan: "Impossible d’analyser la bibliothèque :",
    cantDelete: "Impossible de supprimer :",
    deleted: (path) => `Supprimé : ${path}`,
    truncated: "Limite d’analyse atteinte. Cette liste peut être incomplète.",
    limits: "La recherche s’arrête à six niveaux de dossiers et après environ 15 secondes d’analyse.",
    noRoots: "Aucun dossier n’est disponible pour l’analyse.",
    none: "Aucun jeu trouvé. La recherche descend jusqu’à six niveaux et ignore les dossiers système. D’autres sources restent possibles dans « Modifier » ou « Convertir ».",
    skipped: "Emplacements et fichiers ignorés",
    stays: "Les résultats restent affichés jusqu’à ce que vous cliquiez sur « Actualiser ». Les nouvelles sorties apparaissent après l’actualisation.",
    library: "Bibliothèque",
    storageTip: "Espace libre sur le stockage interne de la console (/data), lors de la dernière analyse",
    free: (free, total) => (
      <>
        Espace libre : <b>{free}</b> sur {total}
      </>
    ),
    refresh: "Actualiser",
    cards: "Vue en cartes",
    list: "Vue en liste",
    games: "Jeux",
    edit: (n) => `Modifier ${n}`,
    convert: (n) => `Convertir ${n}`,
    remove: (n) => `Supprimer ${n}`,
    lz4Tip: "Empaqueté en LZ4 : paquets de ressources ampr_emu",
    lz4Packed: " empaqueté",
  },
  // Lz4.tsx
  lz4: {
    title: "Paquets de ressources LZ4",
    help: "Fonctionnement de LZ4",
    scenario: "Scénario LZ4",
    scenarios: { trace: "Traçage", pack: "Empaquetage/Dépaquetage" },
    actions: { patch: "Patcher", unpatch: "Retirer le patch", pack: "Empaqueter", unpack: "Dépaqueter" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "Enregistre les ressources que le jeu lit pendant que vous jouez, pour l’empaquetage."
      : packed
        ? "Réécrit les paquets LZ4 du jeu sous forme de fichiers ordinaires."
        : "Regroupe les ressources du jeu dans des volumes LZ4 qu’ampr_emu lit pendant le jeu.",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "Recrée cette image avec le runtime de traçage, puis la remplace."
        : "Installe le runtime de traçage dans ce dossier."
      : image
        ? "Recrée cette image avec le runtime de production, puis la remplace."
        : "Remet le runtime de production dans ce dossier.",
    noAmpr: "le jeu n’utilise pas AMPR",
    noEboot: "il n’y a pas d’eboot.bin",
    packedFirst: "la source est empaquetée en LZ4 : dépaquetez-la d’abord",
    damaged: "ses paquets LZ4 sont endommagés",
    archive: "une archive ne peut pas être tracée : convertissez-la d’abord",
    chooseFirst: "choisissez d’abord une source",
    notRead: "la source n’est pas encore lue",
    pickProfile: "Profil LZ4",
    profileFilter: "Profil LZ4 (TOML)",
    pickTracesFolder: "Traces LZ4 (un dossier avec ampr_commands.bin et ampr_emu.index)",
    pickTracesEither: "Traces LZ4 : le zip, ou un dossier contenant les deux fichiers",
    pickTracesZip: "Traces LZ4 (zip)",
    unreadable: "Démarrage impossible : cette source n’a pas pu être lue (voir « Source »).",
    pending: (a) => a === "patch"
      ? "Impossible de patcher pendant qu’une tâche est en cours ou en attente."
      : "Impossible de retirer le patch pendant qu’une tâche est en cours ou en attente.",
    replacing: "Une tâche recrée déjà cette image : attendez qu’elle se termine.",
    blocked: (a, first, more) =>
      `${a === "patch" ? "Impossible de patcher" : a === "unpatch" ? "Impossible de retirer le patch" : a === "pack" ? "Impossible d’empaqueter" : "Impossible de dépaqueter"} : ${first}${more > 0 ? ` (et ${num(more)} de plus, voir « Source »)` : ""}.`,
    chooseGame: "Choisissez d’abord un dossier de jeu ou une image.",
    noAmprGame: "Ce jeu n’utilise pas AMPR, donc LZ4 ne peut pas servir ici.",
    additionalContent: "C’est un contenu additionnel (sans eboot.bin), donc LZ4 ne peut pas servir ici.",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `Ce jeu n’a pas d’ampr_emu : Empaqueter installe l’ampr_emu ${version} fourni.`
        : a === "patch"
          ? `Ce jeu n’a pas d’ampr_emu : Patcher installe le runtime de traçage fourni (ampr_emu ${version}).`
          : `Ce jeu n’a pas d’ampr_emu : Retirer le patch installe le runtime de production fourni (ampr_emu ${version}).`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "ampr_emu.index ne liste pas les fichiers de ce jeu" : state === "damaged" ? "ampr_emu.index est illisible" : "Ce jeu n’a pas d’ampr_emu.index"} : ${a === "pack" ? "Empaqueter" : a === "patch" ? "Patcher" : "Retirer le patch"} en écrit un nouveau.`,
    unavailable: (items) => `Indisponible pour cette source : ${items.join(" ; ")}.`,
    readOnly: (kind) => <>
      <b>{kind}</b> est en lecture seule sur la console, il ne peut donc pas être tracé. Convertissez-le d’abord en <b>ffpkg</b> ou <b>exfat</b>.
    </>,
    action: "Action",
    traceAction: "Action de traçage",
    changes: (a, image) => a === "patch"
      ? `Modifie ${image ? "cette image" : "ce dossier"} : le runtime de traçage remplace fakelib/libSceAmpr.sprx, sans copie de sauvegarde.`
      : `Modifie ${image ? "cette image" : "ce dossier"} : le runtime de production revient et les traces sont supprimées, sans copie de sauvegarde.`,
    replay: (web) => `Jouez, puis choisissez de nouveau cette source pour utiliser ses traces.${web ? " « Traçage » les propose aussi en zip." : ""}`,
    room: "Place pour les traces",
    roomHint: "Espace libre pour les traces du jeu. Montez l’image avec image_rw=, sinon rien n’est enregistré.",
    alreadyRelease: "Déjà sur le runtime de production : seul l’index est réécrit.",
    spaceHint: "Nécessite un espace libre d’environ la taille de l’image ; l’image n’est remplacée qu’une fois la copie vérifiée.",
    deletesTraces: (a, bytes, web) =>
      `${a === "patch" ? "Patcher" : "Retirer le patch"} supprime les traces de cette source (${bytes}) : ${web ? "téléchargez-les d’abord." : "copiez-les d’abord."}`,
    useProfile: "Utiliser plutôt un profil de règles",
    profile: "Profil",
    choose: "Choisir",
    profileHint: "Un fichier TOML de règles d’empaquetage, utilisé à la place des traces.",
    target: "Cible",
    packInto: "Empaqueter vers",
    direct: "Empaquette directement dans l’image ; les fichiers du jeu sont lus deux fois.",
    targetFormat: "Format cible",
    unpackFolder: "Un nouveau dossier de jeu, avec les paquets réécrits sous forme de fichiers ordinaires.",
    unpackHint: (n) =>
      n === null
        ? "Réécrit les fichiers empaquetés sous forme de fichiers ordinaires."
        : `Réécrit ${plural(n, { one: `${num(n)} fichier empaqueté sous forme de fichier ordinaire`, many: `${num(n)} de fichiers empaquetés sous forme de fichiers ordinaires`, other: `${num(n)} fichiers empaquetés sous forme de fichiers ordinaires` })}.`,
    likelyFail: (fmt) => <>
      Risque d’échec en <b>{fmt}</b> : ce format refuse des noms de fichiers listés dans « Source ».
    </>,
    collides: (a, folder) =>
      `Une tâche écrit déjà ${folder ? "ce dossier" : "ce fichier"} : « ${a === "pack" ? "Empaqueter" : "Dépaqueter"} » a besoin d’un autre nom.`,
    already: (a, fmt) => <>
      Ce jeu est déjà en cours d’écriture en <b>{fmt}</b> ; relancer « {a === "pack" ? "Empaqueter" : "Dépaqueter"} » en fait une seconde copie.
    </>,
    profileRules: "Empaquette ce que disent les règles du profil ; les fichiers que le moteur ouvre directement restent tels quels.",
    chosenRules: "Empaquette les fichiers que le jeu a lus dans ces traces ; les fichiers que le moteur ouvre directement restent tels quels.",
    ownRules: "Empaquette les fichiers que le jeu a lus lors de sa dernière session tracée ; les fichiers que le moteur ouvre directement restent tels quels.",
    guessRules: "Aucune trace : l’empaquetage suit une estimation intégrée, moins fiable. Tracez d’abord, ou choisissez des traces ou un profil.",
    recorded: "Traces enregistrées",
    recordedHint: (bytes) => `ampr_commands.bin (${bytes}) : l’empaquetage utilise ces traces.`,
    downloadHint: "Fermez le jeu et attendez une minute, puis téléchargez depuis le navigateur d’un ordinateur. Choisissez-y le zip dans « Empaquetage/Dépaquetage » → « Traces ».",
    download: (bytes, zip) => `Télécharger les traces (${bytes}) sous le nom ${zip}`,
    missingIndex: "Pas d’ampr_emu.index dans cette source : l’empaquetage en a besoin à côté d’ampr_commands.bin.",
    truncatedJournal: "Un journal coupé à la fin s’empaquette quand même, jusqu’à son dernier enregistrement complet.",
    fakelibWarning: (recorded) => recorded
      ? "Peu ou pas de traces ? Désactivez l’outil de mise à jour de fakelib, patchez de nouveau et rejouez."
      : "Désactivez l’outil de mise à jour de fakelib pendant le traçage, sinon rien n’est enregistré.",
    emptyTitle: "Choisissez un jeu qui utilise AMPR",
    experimental: "Expérimental : gardez une copie du jeu d’origine.",
    chooseSource: "Choisir la source",
    source: "Source",
    traces: "Traces",
    optionalTraces: "Traces (facultatif)",
    unlock: "Déverrouiller pour choisir d’autres traces",
    loaded: "Les traces sont déjà chargées depuis cette source",
    lockedHint: (bytes) => `Les traces propres à cette source (${bytes}). Cliquez sur le cadenas pour en choisir d’autres.`,
    clear: "Effacer",
    zip: "Zip",
    folder: "Dossier",
    tracesHint: "Choisissez le zip amprtrace obtenu via « Télécharger les traces », ou un dossier contenant les deux fichiers de traces.",
    useOwn: "Utiliser les traces de la source",
    patchResult: (a, n) =>
      `${a === "patch" ? "Patché : le runtime de traçage est installé" : "Patch retiré : le runtime de production est de retour"} ; l’index liste ${num(n)} ${plural(n, FILES)}.`,
    removed: (files) => `Anciens fichiers de traces supprimés : ${files.join(", ")}.`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "Fonctionnement de LZ4",
    close: "Fermer",
    warning: "Expérimental, uniquement pour les jeux AMPR. Gardez une copie du jeu d’origine.",
    body: () => (
      <>
        <h3>1. Empaquetage/Dépaquetage</h3>
        <ol>
          <li>
            Choisissez le jeu d’origine, <b>Empaquetage/Dépaquetage</b> et les traces (ou un profil).
          </li>
          <li>
            Écrivez un dossier empaqueté ou une image simple ; un dossier se convertit dans n’importe quel format via <b>Convertir</b>.
          </li>
          <li>
            Pour une source empaquetée, <b>Dépaqueter</b> s’affiche à la place : les fichiers redeviennent ordinaires.
          </li>
          <li>Empaqueter garde l’ampr_emu du jeu s’il est de la version fournie ou plus récent et lit les paquets LZ4 ; sinon il installe celui de production fourni, même à la place du runtime de traçage LZ4 et dans un jeu qui n’en a pas.</li>
        </ol>

        <h3>2. Traçage</h3>
        <ol>
          <li>
            Choisissez le <b>ffpkg</b> ou l’<b>exfat</b> du jeu, puis <b>Traçage</b> → <b>Patcher</b>.
          </li>
          <li>Désactivez l’outil de mise à jour de fakelib : il écrase le runtime de traçage.</li>
          <li>
            Montez l’image avec <span className="path">image_rw=</span>, jouez une session, fermez le jeu.
          </li>
          <li>
            Cliquez sur <b>Télécharger les traces</b> sur la page de la PS5, ou copiez ampr_commands.bin et ampr_emu.index.
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    drives: "Lecteurs",
    location: "Emplacement",
    parent: "Dossier parent",
    allDrives: "Tous les lecteurs",
    truncated: (n) => `Seules les ${num(n)} premières entrées sont affichées.`,
    loading: "Chargement",
    noDrives: "Aucun lecteur trouvé.",
    noFolders: "Aucun dossier ici.",
    noImages: "Aucun dossier, image PS5 ni archive ici.",
    noFiles: (ext) => `Aucun dossier ni fichier ${dotted(ext, " ou ")} ici.`,
    chooseFiles: (n) => `Choisir ${num(n)} ${plural(n, FILES)}`,
    nothing: "Aucun dossier ni fichier ici.",
    cancel: "Annuler",
    chooseFolder: "Choisir ce dossier",
  },
  // common.tsx
  common: {
    folder: "Dossier",
    pickFolder: "Dossier du jeu",
    pickImage: "Image ou archive du jeu",
    pickImageFilter: "Images et archives",
    pickFolderButton: "Dossier",
    pickImageButton: "Image/Archive",
    source: "Source",
    emptyTitle: "Choisissez un dossier de jeu, une image ou une archive",
    emptyText: "Un dossier de jeu PS5 ou une image, un paquet ou une archive de ce jeu.",
    accepted: "Sources acceptées",
    unknownTitle: "Titre inconnu",
    additionalContent: "Contenu additionnel",
    reading: "Lecture de la source",
    unreadable: "Source illisible",
    backport: "Backport",
    emulators: "Émulateurs",
    declares: (fw) => `déclare ${fw}`,
    fwUnknown: "firmware inconnu d’après ses exécutables (aucun SDK lisible)",
    emulatorList: (names) => `émulateurs : ${names}`,
    otherEmulator: "Autre",
    sdk: (v) => `SDK ${v} (déclaré)`,
    titleId: "ID du titre",
    version: "Version",
    firmware: "Firmware",
    fwDeclared: "déclaré",
    fwBackport: "backport",
    fwExecutables: "exécutables",
    size: "Taille",
    files: (n) => `${num(n)} ${plural(n, FILES)}`,
    contentId: "ID du contenu",
    noAmpr: "eboot.bin n’importe pas libSceAmpr",
    runtime: {
      forge_release: "Forge (production)",
      forge_trace: "Forge (traçage)",
      other: "autre",
      none: "aucun",
    },
    packed: "Empaqueté",
    damaged: "Paquets endommagés",
    traced: "Tracé",
    plain: "Non empaqueté",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} dans ${num(volumes)} ${plural(volumes, { one: "volume", many: "de volumes", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)} % de la taille`),
    journal: (size) => `journal ${size}`,
    noJournal: "pas encore de journal",
    runtimeIs: (label) => `runtime ${label}`,
    index: {
      stale: "ampr_emu.index périmé : une conversion le renouvelle",
      damaged: "ampr_emu.index endommagé : une conversion le réécrit",
      missing: "pas d’ampr_emu.index : une conversion en écrit un",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `une conversion installe l’ampr_emu ${version} fourni`
        : a === "replace"
          ? `une conversion remplace le runtime par l’ampr_emu ${version} fourni`
          : `ampr_emu ${version} : une conversion le garde`,
    backportLibs: (exclusive, n) =>
      `${exclusive ? "fakelib2/, monté seul" : "fakelib/"}, ${num(n)} ${plural(n, { one: "bibliothèque", many: "de bibliothèques", other: "bibliothèques" })} de backport`,
    findings: "Résultats du contrôle préalable",
    dlcTitle: (n) => `DLC intégrés (${num(n)})`,
    dlcLabel: "DLC intégrés",
    dlcAll: (note) => `Pour chaque DLC : ${note}.`,
    dlcIn: (folder, size) => `dans ${folder}/, ${size}`,
    dlcNoData: "débloqué par l’émulateur de DLC (dlc_emu.ini) : droit d’accès seul, sans données propres",
    dlcStatus: (status) => `débloqué par l’émulateur de DLC (dlc_emu.ini), statut ${status || "non défini"}`,
    dlcMerged: "intégré aux dossiers du jeu",
    backportFiles: "Fichiers du backport",
    emulatorsIn: "Émulateurs dans fakelib",
    qr: (url) => `Code QR : ${url}`,
    needsPassword: "Cette archive demande un mot de passe.",
    enterPassword: "Saisir le mot de passe",
    inArchive: (ext, size) => `dans une archive ${ext} · ${size}`,
    archive: "Archive",
    parts: (n) => `${num(n)} ${plural(n, { one: "partie", many: "de parties", other: "parties" })}`,
    archiveRoot: (root) => `jeu dans ${root}`,
    archiveAtRoot: "jeu à la racine de l’archive",
    solid: "solide",
    encrypted: "protégée par mot de passe",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "Supprimer ce dossier ?" : "Supprimer cette image ?"),
    text: (folder) =>
      folder
        ? "Le dossier et tout son contenu seront supprimés définitivement : pas de corbeille, pas d’annulation. Si la suppression s’arrête en cours de route, une partie peut déjà avoir disparu."
        : "Le fichier image sera supprimé définitivement : pas de corbeille, pas d’annulation. Si la suppression s’arrête en cours de route, une partie peut déjà avoir disparu.",
    no: "Non",
    yes: "Oui, supprimer",
    archiveTitle: "Supprimer cette archive ?",
    archiveText:
      "Le fichier d’archive sera supprimé définitivement : pas de corbeille, pas d’annulation. Les autres parties d’une archive en plusieurs parties restent telles quelles.",
  },
  // password.tsx
  password: {
    title: "Cette archive demande un mot de passe",
    text: "Saisissez son mot de passe pour l’ouvrir.",
    wrong: "Le mot de passe est incorrect.",
    label: "Mot de passe",
    cancel: "Annuler",
    unlock: "Déverrouiller",
    checking: "Vérification…",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "Impossible de joindre le chargeur de payloads sur le port 9021. Lancez d’abord votre autoloader (jailbreak), puis rouvrez Dump Forge.",
      missing:
        "Aucune copie enregistrée de Dump Forge n’a été trouvée (ni dans /data/dumpforge/, ni sur un support USB). Chargez-le une fois depuis un PC : il enregistre une copie pour cette icône.",
      timeout:
        "Dump Forge a été lancé mais ne répond toujours pas après 30 secondes. Il est peut-être encore en cours de démarrage (une console qui sort du mode repos est lente).",
      stopping: "Le Dump Forge précédent est encore en train de s’arrêter (il annule ses tâches et fait le ménage). Patientez un instant, puis réessayez.",
    },
    loadFailed: (e) => `Impossible de charger la page : ${e}`,
    noAnswer: "Dump Forge ne répond pas",
    cantStart: "Impossible de lancer Dump Forge",
    retry: "Réessayer",
    starting: "Lancement de Dump Forge…",
    client: {
      unreachable:
        "Impossible de joindre Dump Forge. Vérifiez que le payload tourne toujours et que cet appareil est sur le même réseau.",
      noPlatform: "Impossible de déterminer si Dump Forge tourne sur une PS5. Actualisez la page pour réessayer.",
      quitUnreachable: "Impossible de joindre Dump Forge pour l’arrêter.",
      stillAnswering: "Dump Forge répond toujours après 2 minutes ; vérifiez la PS5.",
      gone: "Dump Forge ne signale plus cette tâche (retirée de son historique avant que son résultat ait été vu)",
    },
  },
  // target.tsx
  target: {
    format: {
      folder: { text: "Extraire une image ou un paquet vers un dossier de jeu ordinaire." },
      exfat: {
        text: "Pour les jeux qui fonctionnent mal en ffpkg et ne fonctionnent que s’ils sont traités comme du contenu de disque externe. S’ouvre aussi sur Mac ou PC.",
      },
      ffpkg: {
        tone: "good",
        text: "Recommandé. Le système de fichiers natif de la PS5 (UFS2). Montage en écriture possible.",
      },
      ffpfs: {
        tone: "warn",
        text: "Expérimental. Le système de fichiers PFS de la PS5, sans compression. Les noms de fichiers doivent être en ASCII simple.",
      },
      ffpfsc: {
        tone: "warn",
        text: "Expérimental. Le plus compact : un conteneur compressé autour d’une image, toujours monté en lecture seule.",
      },
      pkg: {
        text: "S’installe comme un jeu du Store et tourne à vitesse native. Nécessite kstuff, fpkg-enable et ppr-patch.",
      },
      lz4: {
        tone: "warn",
        text: "Expérimental. Un dossier de jeu dont les ressources sont empaquetées dans des volumes LZ4, que l’émulateur AMPR fourni (ampr_emu) dépaquette pendant le jeu. Uniquement pour les jeux qui utilisent AMPR.",
      },
    },
    innerFfpkg: { tone: "good", text: "Recommandé. Le système de fichiers natif de la PS5 (UFS2)." },
    level0: { text: "Sans compression : chaque bloc est stocké tel quel. Le fichier a la taille de l’image." },
    levelDefault: {
      tone: "good",
      text: "Recommandé. Il s’agit du niveau par défaut de zlib. Le niveau 1 crée environ 3 fois plus vite un fichier environ 3 % plus gros ; le niveau 9 produit un fichier à peine plus petit.",
    },
    levelLow: {
      text: "Création plus rapide qu’au niveau 6 (environ 3 fois au niveau 1) ; le fichier est plus gros (environ 3 % au niveau 1).",
    },
    levelHigh: {
      text: "À peine plus petit qu’au niveau 6 (0,04 % au niveau 9), mais la compression prend plus de temps (environ 25 % au niveau 9), avec tous les cœurs occupés du début à la fin.",
    },
    kraken: {
      fast: { label: "Rapide", note: { tone: "good", text: "Recommandé. La création la plus rapide." } },
      balanced: {
        label: "Équilibré",
        note: {
          text: "Environ 2,6 % plus petit qu’avec « Rapide », mais la compression prend environ 6 fois plus de temps, avec tous les cœurs occupés en permanence.",
        },
      },
      smallest: {
        label: "Le plus petit",
        note: {
          text: "Environ 2,7 % plus petit qu’avec « Rapide », mais la compression prend environ 9 fois plus de temps, avec tous les cœurs occupés en permanence.",
        },
      },
      stored: {
        label: "Sans compression",
        note: {
          text: "Le paquet a à peu près la taille du jeu. À essayer si un paquet compressé s’installe mais ne démarre pas (signalé sur les firmwares 8.60 et 10.40).",
        },
      },
    },
    pkgFirmware: "Fonctionne uniquement avec un firmware 11.60 ou inférieur.",
    imageInside: "Image interne",
    imageInsideLabel: "Image à l’intérieur du ffpfsc",
    compressionLevel: "Niveau de compression",
    compression: "Compression",
    fullVerify: "Vérification complète",
    fullVerifyHint: "Relit chaque octet pour contrôler la sortie. Plus long.",
    separator: "Un nom ne peut pas contenir de séparateur de dossiers ; utilisez « Modifier » pour le dossier.",
    badName: "Un nom ne peut pas être vide, commencer par un point, finir par un point ou une espace, ni contenir de caractères de contrôle.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Trop long : ${bytes} octets, ${max} au maximum (UTF-8).`
        : `Trop long : ${bytes} octets avant l’extension, ${max} au maximum (UTF-8).`,
    pkgExt: (ext) => `Se termine par .${ext}, mais un fpkg est enregistré en .pkg : l’extension détermine le pilote.`,
    otherExt: (ext, target) =>
      `Se termine par .${ext}, mais la cible est ${target} : l’extension détermine le pilote.`,
    extractInto: "Extraire dans ce dossier",
    saveInto: "Enregistrer dans ce dossier",
    output: "Sortie",
    change: "Modifier",
    naming: "Nommage…",
    chooseSourceFirst: "Choisissez d’abord une source",
    fileName: "Nom du fichier",
    savedAs: (name, dir) => `enregistré sous ${name} dans ${dir}`,
    inDir: (dir) => `dans ${dir}`,
    generate: "Générer un nom d’après le contenu",
    generatePattern: "[nom du jeu]-[ID du titre]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} est disponible`,
    update: "Mettre à jour",
    later: "Plus tard",
    updating: "Mise à jour…",
    updatingNote: "La nouvelle version est en cours de téléchargement, l’app redémarrera toute seule.",
    downloading: "Téléchargement dans votre navigateur : décompressez l’archive et remplacez cette copie par son contenu.",
    failed: (v) => `Impossible de mettre à jour vers la v${v}`,
    timeout: (v) => `La v${v} n’a pas démarré en une minute. Si elle a été téléchargée, arrêtez Dump Forge et rouvrez-le pour terminer la mise à jour.`,
    retry: "Réessayer",
    close: "Fermer",
  },
  // api.ts: prettyDuration's units
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default fr;
