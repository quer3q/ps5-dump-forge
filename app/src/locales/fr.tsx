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
    screens: "Écrans",
    active: (n) => `, ${num(n)} en cours`,
    edit: "Modifier",
    storage: "Stockage",
    connect: "Connexion",
    disconnected: "Déconnecté",
    connected: (n: number) => plural(n, { one: `${num(n)} appareil`, many: `${num(n)} d’appareils`, other: `${num(n)} appareils` }),
    about: "À propos",
    offline: "Hors ligne, nouvelle tentative…",
    stop: "Arrêter",
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
    quitting: "Fermeture de PS5 Dump Forge…",
    couldntStop: "Impossible d’arrêter Dump Forge",
    close: "Fermer",
    tryAgain: "Réessayer",
    stopped: "Dump Forge est arrêté",
    startAgain: (ps5) => (ps5 ? "Pour continuer, rouvrez la vignette Dump Forge sur la PS5." : "Pour continuer, relancez la même commande ps5-dump-forge serve."),
    confirmStop: (web: boolean) => (web ? "Arrêter Dump Forge ?" : "Quitter PS5 Dump Forge ?"),
    stopNote: (web, ps5) => (!web ? "Aucune tâche en cours." : ps5 ? "Pour le relancer, ouvrez la vignette Dump Forge sur la PS5." : "Pour le relancer, exécutez de nouveau la même commande ps5-dump-forge serve."),
    peerBusy: "Un appareil associé lit ou écrit des fichiers sur cet ordinateur. Quitter interrompt son transfert.",
    quit: "Quitter",
    github: (name) => `${name} sur GitHub`,
    queued: "en attente",
    progress: (percent, left) => `${percent} % terminé${left ? `, il reste ${left}` : ""}`,
  },
  // build/ (Build's convert settings and checks)
  convert: {
    targetFormat: "Format",
    alreadyFolder: "la source est déjà un dossier",
    patchBlocked: "Création impossible : c’est un paquet de correctif, qui ne contient que ce que la mise à jour a changé. Ouvrez-le dans « Modifier » pour en extraire les fichiers.",
    openInEdit: "Ouvrir dans « Modifier »",
    noContentId: "Création en FPKG impossible : le param.json de ce jeu n’a pas de Content ID, indispensable à un paquet.",
    blocked: (first, more) =>
      `Création impossible : ${first}${more > 0 ? ` (et ${num(more)} de plus, voir « Source »)` : ""}.`,
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index ne liste pas les fichiers de ce jeu" : state === "damaged" ? "ampr_emu.index est illisible" : "Ce jeu n’a pas d’ampr_emu.index"} : la conversion en écrit un nouveau.`,
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
      zstd: { tagline: "Avec ampr-emu-ng", cells: ["Chargement plus rapide", "Jusqu’à la moitié de la taille du jeu"] },
      lz4: {
        tagline: "Avec ampr-emu-ng",
        cells: ["Chargement 30–40 % plus rapide, selon les auteurs", "Environ la moitié de la taille du jeu, selon les auteurs"],
      },
      ffpkg: { tagline: "Recommandé pour ShadowMountPlus", cells: ["Lecture de l’image à pleine vitesse", FLAT] },
      exfat: { tagline: "Compatibilité", cells: ["Lecture de l’image à pleine vitesse", FLAT] },
      ffpfs: { tagline: "PFS non compressé", cells: ["Lecture de l’image à pleine vitesse", FLAT] },
      ffpfsc: {
        tagline: "Le plus compact",
        cells: ["150–250 Mo/s, saccades possibles", "Dépend du niveau de compression, en général 30–60 % de moins"],
      },
      folder: { tagline: "Fichiers du jeu tels quels", cells: ["Pleine vitesse du disque", FLAT] },
    },
    compare: "Comparer les formats",
    compareLabel: "Comparaison des formats",
    format: "Format",
    credits: "Crédits",
    upstream: (app: string) =>
      `Chiffres des auteurs : ce qu’annoncent les auteurs d’ampr_emu et de Lazy_AMPR, non mesuré par ${app} ; ils varient selon le jeu.`,
    bundled: "ampr_emu de drakmor (GPL-3.0), dont ampr-emu-ng est un fork. Forge ne le fournit plus : il remplace l’ampr_emu d’un jeu par ampr-emu-ng, sauf si les Réglages le gardent. Source :",
    lazy: "La stratégie d’empaquetage s’appuie sur Lazy_AMPR de Nazky :",
    ng: "Les paquets ZSTD et LZ4 fonctionnent avec ampr-emu-ng (GPL-3.0-or-later), le seul runtime AMPR fourni, sans modification. Source :",
    logo: "Le logo est l’œuvre de Xxcco :",
    unrar:
      "Les archives .rar sont lues avec la bibliothèque UnRAR fournie, d’Alexander Roshal (extraction seule, sous la licence UnRAR).",
    language: "Langue",
    system: "Système",
    notifications: "Notifier à la fin d'une tâche",
    notificationsHint: "S'affiche sur la console à la fin d'une tâche de création, d'empaquetage ou de transfert.",
    notificationsFailed: "Impossible de modifier les notifications.",
  },
  // Edit.tsx
  edit: {
    details: "Détails",
    deleting: "Suppression de la source",
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
    facts: "Infos sur le jeu",
    explorer: "Fichiers du jeu",
    howTitle: "Comment ça marche",
    how: (app) => [
      "Ouvrez le **dossier** ou **l’image** d’un jeu, puis modifiez-le comme un dossier sur une clé USB : **ajoutez et remplacez des fichiers** (glissez-les ici ou utilisez « Ajouter »), **renommez et supprimez**.",
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
    actions: "Actions",
    items: (n) => `${num(n)} ${plural(n, { one: "élément", many: "d’éléments", other: "éléments" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "dossier", many: "de dossiers", other: "dossiers" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "correspondance", many: "de correspondances", other: "correspondances" })}`,
    emptyFolder: "Ce dossier est vide.",
    open: (name) => `Ouvrir ${name}`,
    archiveNote: "Les archives peuvent être parcourues, mais pas modifiées. Pour la modifier, créez-en d’abord un dossier ou une image.",
    rebuildTimePkg: (t) => `Reconstruire compresse à nouveau tout le jeu : environ ${t} à ~26 Mo/s sur une PS5 (plus rapide sur un ordinateur). Modifiez plutôt une copie dépaquetée pour l’éviter.`,
    rebuildTimePs5: (t) => `Reconstruire réécrit toute l’image (environ ${t} sur cette PS5).`,
    rebuildTimeHost: (t) => `Reconstruire réécrit toute l’image (environ ${t} sur cet ordinateur).`,
    rebuildTimeAny: (t) => `Reconstruire réécrit toute l’image (environ ${t} sur une PS5).`,
    nasNote: "Les jeux d’un NAS peuvent être parcourus et extraits, mais pas modifiés. Pour le modifier, créez-le ou extrayez-le d’abord sur cet appareil.",
    patchNote: "C’est un paquet de correctif : il ne contient que ce que la mise à jour a changé. Les fichiers qu’il contient en entier sont listés ; ils peuvent être parcourus et extraits, mais pas modifiés.",
    peerNote: "Les jeux d’un autre Forge peuvent être parcourus et extraits, mais pas modifiés. Pour le modifier, créez-le ou extrayez-le d’abord sur cet appareil.",
    addFiles: "Ajouter des fichiers",
    addFolder: "Ajouter un dossier",
    pickAddFiles: "Choisissez les fichiers à ajouter",
    pickAddFolder: "Choisissez le dossier à ajouter",
    peerRoot: "Un appareil entier ne peut pas être ajouté : choisissez un dossier à l’intérieur.",
    renameItem: (name) => `Renommer ${name}`,
    removeItem: (name) => `Supprimer ${name}`,
    renameTip: "Renommer (F2)",
    removeTip: "Supprimer (touche Suppr)",
    removeTipMac: "Supprimer (touches fn+Supprimer)",
    extractItem: (name: string) => `Extraire ${name}`,
    extractTip: "Copier dans un dossier de votre choix",
    extractFolder: "Extraire le dossier",
    extractFolderTip: "Copier le dossier affiché dans un dossier de votre choix",
    extractGameTip: "Copier tout le jeu dans un dossier de votre choix",
    extractNotOnDisk: "Ce dossier n’existe que dans vos modifications en attente : appliquez-les pour l’extraire.",
    pickExtract: "Choisissez où extraire",
    extractPending: "Extrait les fichiers tels qu’ils sont maintenant, sans vos modifications en attente.",
    nameEmpty: "Saisissez un nom.",
    nameBad: "Un nom ne peut pas contenir / ni \\, ni être . ou ..",
    nameTaken: (name) => `${name} existe déjà ici.`,
    markNew: "Nouveau",
    markReplaced: "Remplacé",
    markRenamed: "Renommé",
    wasAt: (path) => `Avant : ${path}`,
    sizeLater: "Plus que ce qui a pu être listé : le reste est lu quand les modifications sont appliquées",
    dropInto: (folder) => `Déposez pour ajouter à ${folder}`,
    droppedGone: (path) => `${path} : le fichier déposé n’est plus là`,
    pending: (n) =>
      plural(n, {
        one: `${num(n)} modification pas encore appliquée`,
        many: `${num(n)} de modifications pas encore appliquées`,
        other: `${num(n)} modifications pas encore appliquées`,
      }),
    undo: "Annuler",
    undoTip: "Annuler la dernière action, avec toutes ses modifications",
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
    modeLabel: "Mode d’application",
    checking: "Vérification de la façon d’appliquer les modifications",
    nothingToChange: "Rien à changer : le jeu reste tel quel.",
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
    alwaysRebuilt: "Les images ffpfsc et FPKG sont toujours reconstruites.",
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
    uploadingFile: (i: number, n: number) => `Envoi du fichier ${num(i)} sur ${num(n)}`,
    uploading: (i: number, n: number, sent: string, total: string) => `Envoi du fichier ${num(i)} sur ${num(n)} : ${sent} sur ${total}`,
    uploadLabel: "Envoi",
    uploadCancelled: "Envoi annulé. Rien n’a changé dans le jeu.",
    applyRunning: "Application des modifications. Vous pourrez de nouveau modifier une fois la tâche terminée (voir « File d’attente »).",
    applyFailed: "Les modifications n’ont pas été appliquées :",
    applyRefused: "Les modifications ne peuvent pas être appliquées ainsi :",
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
      install: "Installation",
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
      install: "Installation",
    },
    stepsLabel: "Étapes",
    stepDone: (s) => `Terminé : ${s}`,
    stepFailed: (s) => `Échec : ${s}`,
    stepLater: (s) => `À venir : ${s}`,
    packingWriting: "Empaquetage et écriture",
    rate: { read: "lecture", compress: "compression", pack: "empaquetage", write: "écriture" },
    showPath: "Afficher le chemin",
    showFinder: "Afficher dans le Finder",
    showExplorer: "Afficher dans l’Explorateur",
    showFolder: "Afficher dans le dossier",
    packTag: {
      lz4Pack: (level: string) => `LZ4 empaquetage · ${level}`,
      lz4Unpack: "LZ4 dépaquetage",
      zstdPack: (level: string) => `ZSTD empaquetage · ${level}`,
      level: (n: number) => `niveau ${num(n)}`,
      zstdUnpack: "ZSTD dépaquetage",
      withRules: (tag) => `${tag} · règles personnalisées`,
    },
    edit: "Modification",
    editRebuild: "Modification · reconstruction",
    transfer: "Transfert",
    extract: "Extraction",
    job: (id) => `Tâche ${id}`,
    done: "Terminé",
    cancelled: "Annulé",
    failed: "Échec",
    cancelling: "Annulation…",
    queued: "En attente",
    dismiss: "Masquer",
    retry: "Réessayer",
    retrying: "Démarrage",
    stopJob: "Arrêter ?",
    cancel: "Annuler",
    stopHint: "Appuyez de nouveau sur « Arrêter ? » dans les 4 secondes pour arrêter la tâche.",
    remoteLabel: (name, device) => `${name}, une tâche sur ${device}`,
    remoteTitle: (device) => `S’exécute sur ${device} et utilise les fichiers de cet appareil : ici, vous pouvez seulement l’annuler`,
    from: "depuis ",
    left: (d) => `il reste ${d}`,
    inStep: (time) => `${time} dans cette étape`,
    outputPath: "Chemin de sortie",
    sourceDeleted: (v) => `Source supprimée : ${v}`,
    sourceKept: (v) => `Source conservée : ${v}`,
    sourcePartlyDeleted: (v) => `Source supprimée en partie : ${v}`,
    checks: "Contrôles",
    verifiedPlain: (n) => `Vérifié : ${num(n)} ${plural(n, CHECKS)} ${plural(n, { one: "réussi", many: "réussis", other: "réussis" })}`,
    verifiedFull: (n) => `Vérification complète réussie : ${num(n)} ${plural(n, CHECKS)}`,
    verifiedFast: (n) => `Vérification rapide réussie : ${num(n)} ${plural(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `Données vérifiées : ${checked} sur ${total}, en ${num(n)} ${plural(n, { one: "échantillon", many: "d’échantillons", other: "échantillons" })}`,
    cantBuild: "Impossible de créer ce jeu :",
    cantEdit: "Impossible d’appliquer les modifications :",
    cantExtract: "Impossible d’extraire :",
    cantTransfer: "Transfert impossible :",
    cantBuildAs: (f) => (
      <>
        Impossible de créer ce jeu en <b>{f}</b> :
      </>
    ),
    log: (n) => `Journal (${num(n)})`,
    logLabel: "Journal",
  },
  // Storage text shared by Files.tsx, Packages.tsx, Edit.tsx and common.tsx (FreeSpace)
  library: {
    cantDelete: "Impossible de supprimer :",
    deleted: (path) => `Supprimé : ${path}`,
    storageTip: "Espace libre sur le stockage interne de la console (/data)",
    free: (free, total) => (
      <>
        Espace libre : <b>{free}</b> sur {total}
      </>
    ),
    cards: "Vue en cartes",
    list: "Vue en liste",
    remove: (n) => `Supprimer ${n}`,
    deleting: (n) => `Suppression de ${n}`,
    showAs: "Afficher en",
  },
  // Storage.tsx, Files.tsx: the Storage tab (File explorer | Package installer)
  storage: {
    views: "Vue du stockage",
    files: "Explorateur de fichiers",
    title: "Explorateur de fichiers",
    roots: "Lecteurs",
    location: "Emplacement",
    folders: "Dossiers",
    up: "Remonter",
    upTip: "Remonter d’un dossier",
    refresh: "Actualiser",
    filter: "Filtrer par nom",
    filterLabel: "Filtrer ce dossier par nom",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "dossier", many: "de dossiers", other: "dossiers" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "correspondance", many: "de correspondances", other: "correspondances" })}`,
    firstOf: (max, n) => `${num(max)} premiers sur ${num(n)}`,
    truncated: "Ce dossier contient plus de 10 000 éléments : seuls les 10 000 premiers sont listés.",
    name: "Nom",
    size: "Taille",
    emptyFolder: "Ce dossier est vide.",
    noMatch: (f) => `Rien ici ne correspond à « ${f} ».`,
    noRoots: "Aucun lecteur n’est disponible.",
    cantList: "Impossible d’ouvrir ce dossier :",
    link: "Lien",
    linkTip: "Un lien : il peut être ouvert, mais pas renommé, déplacé ni supprimé ici",
    mounted: "Monté",
    mountedTip: "Monté par ShadowMount+ : quittez le jeu et désactivez ShadowMount+ pour le supprimer, le renommer ou le déplacer",
    mountedNote: "Les images marquées Monté sont utilisées par ShadowMount+ : quittez le jeu et désactivez ShadowMount+ pour les supprimer, les renommer ou les déplacer.",
    open: (n) => `Ouvrir ${n}`,
    more: (n) => `Autres actions pour ${n}`,
    rename: (n) => `Renommer ${n}`,
    renameTip: "Renommer (F2)",
    move: (n) => `Déplacer ${n}`,
    moveTip: "Déplacer vers un autre dossier",
    moveTitle: (n) => `Déplacer ${n} vers`,
    remove: (n) => `Supprimer ${n}`,
    removeTip: "Supprimer (touche Suppr)",
    removeTipMac: "Supprimer (touches fn+Supprimer)",
    cantDelete: "Impossible de supprimer :",
    cantRename: "Impossible de renommer :",
    cantMove: "Impossible de déplacer :",
    sameFolder: "Il se trouve déjà dans ce dossier.",
    deleted: (path) => `Supprimé : ${path}`,
    renamed: (path) => `Renommé en ${path}`,
    moved: (path) => `Déplacé vers ${path}`,
    devices: "Appareils",
    thisDevice: "Cet appareil",
    otherDevice: "« Déplacer » reste sur un seul appareil : l’écran « Transfert » copie d’un appareil à l’autre.",
    loadingDevices: "Chargement des appareils",
    noDevice: "Aucun appareil connecté",
    noDeviceHint: "Associez une PS5 ou un ordinateur, ou ajoutez un NAS, pour gérer ses fichiers ici.",
    openConnect: "Ouvrir « Connexion »",
    dropMove: (folder) => `Déposez pour déplacer dans ${folder}`,
    cantCopy: "Impossible de copier :",
    copyNoFolder: "Ouvrez d’abord un dossier sur cet appareil.",
    copyOne: (name: string, to: string) => `Copie de ${name} vers ${to}. Le transfert apparaît dans « File d’attente ».`,
    copyMany: (n: number, to: string) =>
      plural(n, {
        one: `Copie de ${num(n)} élément vers ${to}. Le transfert apparaît dans « File d’attente ».`,
        many: `Copie de ${num(n)} d’éléments vers ${to}. Le transfert apparaît dans « File d’attente ».`,
        other: `Copie de ${num(n)} éléments vers ${to}. Le transfert apparaît dans « File d’attente ».`,
      }),
  },
  // build/ (Build's pack settings and checks), JobList.tsx's pack tags
  pack: {
    help: "Fonctionnement de l’empaquetage",
    patch: "Un paquet de correctif ne peut pas être empaqueté.",
    additionalContent: "Un contenu additionnel (sans eboot.bin) ne peut pas être empaqueté.",
    levels: { fast: "Rapide", medium: "Moyen", max: "Maximum" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Recommandé. ZSTD 5 : rapide, bonne taille." },
        medium: { text: "ZSTD 9 : plus petit, plus lent." },
        max: { text: "ZSTD 19 : le plus petit ; bien plus lent, plus de mémoire." },
      },
      lz4: {
        fast: { tone: "good", text: "Recommandé. LZ4 par défaut : le plus rapide." },
        medium: { text: "LZ4 HC 9 : plus petit, plus lent." },
        max: { text: "LZ4 HC 12 : le plus petit, le plus lent." },
      },
    },
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index est périmé" : state === "damaged" ? "ampr_emu.index est endommagé" : "Pas d’ampr_emu.index"} : l’empaquetage en écrit un nouveau.`,
    rules: {
      title: "Règles personnalisées (TOML)",
      titleWith: (name) => `Règles personnalisées (TOML) : ${name}`,
      info: "Un profil de la stratégie d’empaquetage au format de ampr_pack.py d’ampr_emu et de Lazy_AMPR décide quels fichiers sont empaquetés et lesquels restent à part. La dernière règle correspondante l’emporte ; les fichiers système restent toujours à part.",
      warn: "De mauvaises règles peuvent donner un jeu qui ne démarre pas. Gardez une copie.",
      pickTitle: "Stratégie d’empaquetage (fichier TOML)",
      clear: "Effacer",
    },
  },
  // PackHelp.tsx
  packHelp: {
    title: "Fonctionnement de l’empaquetage",
    close: "Fermer",
    body: () => (
      <>
        <h3>ZSTD</h3>
        <ol>
          <li>Empaquette les ressources du jeu dans des volumes ZSTD, dans un dossier ou une image.</li>
          <li><b>Rapide</b> (ZSTD 5) va vite ; <b>Moyen</b> (9) et <b>Maximum</b> (19) sont plus petits, plus lents.</li>
          <li>Pour un jeu empaqueté en ZSTD, <b>Aucun</b> le dépaquette : les fichiers ordinaires reviennent.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Certains jeux risquent de ne pas démarrer.</b> Gardez une copie.</li>
          <li>Empaquette dans des volumes LZ4, dans un dossier ou une image : <b>Rapide</b> (LZ4), <b>Moyen</b> (HC 9), <b>Maximum</b> (HC 12).</li>
          <li>Fonctionne avec ampr-emu-ng, comme les paquets ZSTD.</li>
          <li>Pour un jeu empaqueté en LZ4, <b>Aucun</b> le dépaquette.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>Le seul runtime AMPR fourni avec Forge. Il lit les paquets ZSTD et LZ4 ; l’empaquetage l’installe dans fakelib/.</li>
          <li>À la création, Forge remplace l’ampr_emu d’un jeu par ampr-emu-ng, sauf si les Réglages gardent ampr_emu. Un FPKG ne contient pas de runtime : le chargeur de la console le fournit.</li>
          <li>Désactivez la mise à jour de fakelib de ShadowMount+, sinon elle remplace le runtime et le jeu ne peut plus lire ses paquets.</li>
        </ol>

        <h3>Stratégie d’empaquetage</h3>
        <ol>
          <li><b>Tout empaqueter sauf les fichiers système</b> (ZSTD et LZ4) : tous les fichiers sauf eboot.bin, les exécutables et modules, sce_sys, sce_module, fakelib et les fichiers .ini (dlc_emu.ini reste modifiable) ; ampr-emu-ng sert le reste, audio et vidéo compris.</li>
          <li>À l’empaquetage, sous <b>Règles personnalisées (TOML)</b>, choisissez un profil au format de ampr_pack.py et Lazy_AMPR, sur cet appareil ou un autre.</li>
          <li>La dernière règle correspondante l’emporte ; les fichiers système restent toujours à part. De mauvaises règles peuvent donner un jeu qui ne démarre pas.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "Appareils",
    thisDevice: "Cet appareil",
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
    chooseFiles: (n) => (n === 0 ? "Choisir des fichiers" : `Choisir ${num(n)} ${plural(n, FILES)}`),
    nothing: "Aucun dossier ni fichier ici.",
    cantList: "Impossible d’ouvrir ce dossier :",
    cancel: "Annuler",
    chooseFolder: "Choisir ce dossier",
    netOnlyHere: "Archives et FPKG : depuis cet appareil uniquement.",
  },
  // bookmarks.tsx: the bookmarked folders (Transfer and the File explorer)
  bookmarks: {
    label: "Favoris",
    add: "Ajouter ce dossier aux favoris",
    remove: "Retirer des favoris",
    removeNamed: (name: string) => `Retirer ${name} des favoris`,
    full: "9 favoris au maximum",
  },
  // DeviceTabs.tsx
  deviceTabs: {
    add: "Ajouter un appareil",
    offline: "Hors ligne",
    offlineNamed: (name: string) => `${name} · Hors ligne`,
  },
  // common.tsx
  common: {
    folder: "Dossier",
    pickButton: "Choisir",
    pickAny: "Dossier, image ou archive du jeu",
    source: "Source",
    emptyTitle: "Choisissez un dossier de jeu, une image ou une archive",
    accepted: "Sources acceptées",
    unknownTitle: "Titre inconnu",
    unsigned: (n: number) =>
      plural(n, {
        one: `${num(n)} exécutable n’est pas signé : le jeu ne démarrera pas. La conversion du jeu le signe.`,
        many: `${num(n)} d’exécutables ne sont pas signés : le jeu ne démarrera pas. La conversion du jeu les signe.`,
        other: `${num(n)} exécutables ne sont pas signés : le jeu ne démarrera pas. La conversion du jeu les signe.`,
      }),
    patch: "Correctif",
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
    traceRuntime: "Ancien runtime de traçage LZ4",
    noRuntime: "Aucun runtime",
    packed: (kind) => `Empaqueté (${kind})`,
    zstdKept: "paquets de ressources ZSTD : une conversion les conserve",
    damaged: "Paquets endommagés",
    unpacked: "Non empaqueté",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} dans ${num(volumes)} ${plural(volumes, { one: "volume", many: "de volumes", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)} % de la taille`),
    index: {
      stale: "ampr_emu.index périmé : une conversion le renouvelle",
      damaged: "ampr_emu.index endommagé : une conversion le réécrit",
      missing: "pas d’ampr_emu.index : une conversion en écrit un",
    },
    runtimeAction: (a: "install" | "replace" | "keep" | "keepIfPacks") =>
      a === "install"
        ? "une conversion installe l’ampr-emu-ng fourni"
        : a === "replace"
          ? "une conversion remplace le runtime par l’ampr-emu-ng fourni"
          : a === "keep"
            ? "une conversion le garde (Réglages)"
            : "une conversion le garde s’il lit les paquets LZ4, sinon le remplace par ampr-emu-ng",
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
        : "Le fichier image sera supprimé définitivement : pas de corbeille, pas d’annulation.",
    no: "Non",
    yes: "Oui, supprimer",
    archiveTitle: "Supprimer cette archive ?",
    archiveText:
      "Le fichier d’archive sera supprimé définitivement : pas de corbeille, pas d’annulation. Les autres parties d’une archive en plusieurs parties restent telles quelles.",
    fileTitle: "Supprimer ce fichier ?",
    fileText: "Le fichier sera supprimé définitivement : pas de corbeille, pas d’annulation.",
  },
  // replace.tsx
  replace: {
    title: (n) =>
      plural(n, { one: `${num(n)} élément existe déjà à cet endroit`, other: `${num(n)} éléments existent déjà à cet endroit` }),
    inFolder: (folder) => <>Dans {folder} :</>,
    more: (n) => `et ${num(n)} de plus`,
    text: "« Remplacer » met les nouveaux à leur place : les anciens fichiers sont perdus pour de bon, sans corbeille ni annulation. Un dossier posé sur un dossier du même nom est fusionné : ses fichiers de même nom sont remplacés, tout le reste est conservé.",
    cancel: "Annuler",
    replace: "Remplacer",
  },
  // password.tsx
  password: {
    title: "Cette archive demande un mot de passe",
    text: "Saisissez son mot de passe pour l’ouvrir.",
    wrong: "Le mot de passe n’est pas correct. Vérifiez-le et réessayez.",
    label: "Mot de passe",
    cancel: "Annuler",
    unlock: "Déverrouiller",
    checking: "Vérification",
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
      uploadIncomplete: "L’envoi ne s’est pas terminé.",
    },
  },
  // target.tsx
  target: {
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
    imageInside: "Image interne",
    imageInsideLabel: "Image à l’intérieur du ffpfsc",
    compressionLevel: "Niveau de compression",
    compression: "Compression",
    fullVerify: "Vérification complète",
    fullVerifyHint: "Relit chaque octet pour contrôler la sortie. Plus long.",
    deleteSource: "Supprimer la source",
    deleteSourceHint: "Irréversible. La source n'est supprimée qu'après une conversion réussie.",
    deleteSourceNas: "Un jeu sur un NAS ne peut pas être supprimé d'ici.",
    separator: "Un nom ne peut pas contenir de séparateur de dossiers ; utilisez « Modifier » pour le dossier.",
    badName: "Un nom ne peut pas être vide, commencer par un point, finir par un point ou une espace, ni contenir de caractères de contrôle.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Trop long : ${bytes} octets, ${max} au maximum (UTF-8).`
        : `Trop long : ${bytes} octets avant l’extension, ${max} au maximum (UTF-8).`,
    pkgExt: (ext) => `Se termine par .${ext}, mais un FPKG est enregistré en .pkg : l’extension détermine le pilote.`,
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
  },
  // Formats.tsx (About's Output names) and target.tsx's name pattern: generated output names
  naming: {
    title: "Noms de sortie",
    parts: { title: "Titre", title_id: "ID du titre", version: "Version", pack: "Type de paquet", region: "Région", fw: "Firmware" },
    template: "Modèle de nom",
    partsLabel: "Insérer une partie",
    insert: (part: string) => `Insérer ${part}`,
    inUse: (part) => `${part} est déjà dans le modèle`,
    example: "Exemple",
    length: "Longueur",
    bytes: (n, max) => `${n} octets sur ${max}.`,
    room: (n) => plural(n, { one: `Il reste ${num(n)} octet pour le titre.`, other: `Il reste ${num(n)} octets pour le titre.` }),
    noRoom: "Il ne reste aucune place pour le titre : il est omis.",
    versionOut: "La version est omise pour faire de la place.",
    cut: "Le nom est coupé à 63 octets.",
    idOnly: "Le texte du modèle remplit les 63 octets : l'ID du titre nomme donc la sortie.",
    errors: {
      long: (max: number) => `Un modèle fait au plus ${max} octets.`,
      open: "Une { n'a pas de } fermante.",
      close: "Une } n'a pas de { ouvrante.",
      unknown: (token: string) => `{${token}} n'est pas une partie : utilisez Titre, ID du titre, Version, Type de paquet, Région ou Firmware.`,
      twice: (token) => `{${token}} figure deux fois dans le modèle : utilisez chaque partie une seule fois.`,
      char: (c: string) => `Un nom ne peut pas contenir ${c}.`,
      control: "Un nom ne peut pas contenir de caractère de contrôle.",
      needsName: "Un nom a besoin du Titre ou de l'ID du titre.",
    },
    notSaved: "Non enregistré : les noms suivent toujours le dernier modèle valide.",
    reset: "Réinitialiser",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} est disponible`,
    update: "Mettre à jour",
    later: "Plus tard",
    updating: "Mise à jour…",
    updatingNote: "La nouvelle version est en cours de téléchargement. Cette page se rechargera d’elle-même avec elle.",
    downloading: "Le téléchargement s’ouvre dans votre navigateur : remplacez cette copie par le fichier téléchargé (sur Mac, faites glisser l’app du .dmg vers Applications).",
    downloadingCli: "Le téléchargement s'ouvre dans le navigateur : remplacez la CLI ps5-dump-forge par le fichier téléchargé, ou lancez ps5-dump-forge update --install sur l'ordinateur où elle tourne.",
    failed: (v) => `Impossible de mettre à jour vers la v${v}`,
    timeout: (v) => `La v${v} n’a pas démarré en une minute. Si elle a été téléchargée, arrêtez Dump Forge et rouvrez-le pour terminer la mise à jour.`,
    retry: "Réessayer",
    close: "Fermer",
  },
  // api.ts: prettyDuration's units
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "Cet appareil",
    howItWorks: "Comment se connecter",
    ipAddress: "Adresse IP",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? plural(n, { one: `vu : il y a ${num(n)} minute`, many: `vu : il y a ${num(n)} de minutes`, other: `vu : il y a ${num(n)} minutes` })
        : unit === "h"
          ? plural(n, { one: `vu : il y a ${num(n)} heure`, many: `vu : il y a ${num(n)} d’heures`, other: `vu : il y a ${num(n)} heures` })
          : unit === "d"
            ? plural(n, { one: `vu : il y a ${num(n)} jour`, many: `vu : il y a ${num(n)} de jours`, other: `vu : il y a ${num(n)} jours` })
            : "vu : à l’instant",
    listenError: (e) => `Les autres appareils ne peuvent pas joindre celui-ci : ${e}`,
    unavailable: (e) => `Connexion indisponible : ${e}`,
    loading: "Chargement",
    discover: "Détection automatique",
    stop: "Arrêter",
    fileTransfer: "Transfert de fichiers",
    devices: "Appareils",
    show: "Afficher",
    all: "Tous",
    onlineOf: (n, total) => `${num(n)} sur ${num(total)} en ligne`,
    sendFilesTo: (name) => `Envoyer des fichiers à ${name}`,
    userIs: (user) => `utilisateur ${user}`,
    nearby: "À proximité",
    nearbyNone: "Aucun nouvel appareil trouvé",
    addDevice: "Ajouter un appareil",
    addressExample: "192.168.1.20 ou nas.local",
    addByProtocol: "Ou ajouter par protocole :",
    savedEmpty: "Aucun appareil enregistré pour l’instant",
    savedEmptyHint:
      "La détection automatique trouve Forge et le stockage réseau sur ce réseau. Associez un appareil et autorisez-le sur celui-ci, ou enregistrez un NAS.",
    online: "En ligne",
    offline: "Hors ligne",
    computer: "Ordinateur",
    address: "Adresse",
    keyId: "ID de clé",
    lastSeen: "Vu pour la dernière fois",
    status: "État",
    never: "Jamais",
    unknown: "Inconnu",
    forget: "Oublier",
    forgetNote: "« Oublier » retire l’association sur cet appareil seulement. Pour l’utiliser de nouveau, associez-le à nouveau.",
    close: "Fermer",
    cancel: "Annuler",
    pairTitle: "Associer un appareil",
    pair: "Associer",
    connectTo: "Connecter",
    probing: (address) => `Vérification de ${address}`,
    pairing: "Association",
    waitingFor: (address) => `En attente de l’autorisation de l’association par ${address}`,
    checkNumber: "Numéro de contrôle",
    checkHint: "Il doit correspondre au numéro qu’affiche l’autre appareil.",
    denied: "L’autre appareil a refusé l’association.",
    timedOut: "Personne n’a autorisé l’association sur l’autre appareil à temps.",
    pairBusy: "L’autre appareil a d’autres demandes d’association en attente. Réessayez dans une minute.",
    alreadyPaired: "Cet appareil est déjà enregistré sur l’autre. Oubliez-le d’abord là-bas, puis associez-le à nouveau.",
    tooManyAttempts: "Trop de demandes refusées. Réessayez dans 10 minutes.",
    pairPromptTitle: (name) => `Associer à ${name} ?`,
    pairPromptBody: (name, platform, ip) =>
      `${name} (${platform}, ${ip}) veut s’associer à cet appareil. Un appareil associé peut lire les fichiers de cet appareil et en ajouter de nouveaux.`,
    pairPromptCheck: (name) => `N’autorisez que si ${name} affiche le même numéro de contrôle.`,
    allow: "Autoriser",
    deny: "Refuser",
    notPaired: "L’autre appareil ne connaît plus celui-ci. Associez-le à nouveau.",
    searching: "Recherche sur ce réseau",
    forge: "Forge",
    save: "Enregistrer",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `Ajouter un NAS ${protocol}`,
    nasEditTitle: (protocol) => `Modifier le NAS ${protocol}`,
    nasName: "Nom",
    nasAddressHint: "Adresse IP ou nom d’hôte ; ajoutez :port s’il n’est pas 445",
    nasUser: "Nom d’utilisateur",
    nasUserHint: "Laissez vide pour vous connecter en invité",
    ftpAddressHint: "Adresse IP ou nom d'hôte ; ajoutez :port s'il n'est pas 21",
    nfsAddressHint: "Adresse IP ou nom d'hôte ; ajoutez :port s'il n'est pas 2049",
    ftpUserHint: "Laissez vide pour vous connecter en anonyme",
    ftpNote: "FTP n'est pas chiffré, identifiants compris.",
    nfsNote: "NFS n'a pas d'identifiants : Forge se connecte en root (NFSv4.1, sinon NFSv3), l'export du serveur doit donc autoriser cet appareil. NFS n'est pas chiffré.",
    nasPassword: "Mot de passe",
    nasPasswordKeep: "Laissez vide pour garder le mot de passe enregistré",
    guest: "Invité",
    passwordSaved: "Enregistré",
    passwordNone: "Aucun",
    edit: "Modifier",
    nasForgetNote: "« Oublier » retire ce NAS et son mot de passe enregistré de cet appareil.",
    thisBrowser: "Ce navigateur",
    uploadPick: "Choisir les fichiers à envoyer",
    uploading: (name, i, n) => `Envoi de ${name} (${num(i)} sur ${num(n)})`,
    uploaded: "Envoi terminé",
    uploadSkipped: (names) => `Non envoyé, un dossier ou un lien de ce nom existe à cet endroit : ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `${num(done)} fichier sur ${num(n)} a été envoyé et reste sur cet appareil.`,
        other: `${num(done)} fichiers sur ${num(n)} ont été envoyés et restent sur cet appareil.`,
      }),
    cantUpload: "Impossible d’envoyer :",
    send: "Envoyer",
    downloadHere: "Télécharger depuis cet appareil",
    wholeRoot: "Un appareil, un partage ou un lecteur entier ne peut pas être envoyé : choisissez un dossier à l’intérieur.",
    helpPhone: "Scannez le QR code pour ouvrir cette page sur un téléphone. Là, « Transfert » envoie les fichiers du téléphone vers cet appareil et télécharge ceux de cet appareil sur le téléphone.",
    help: [
      "Connexion relie Dump Forge sur votre PS5 et vos ordinateurs d’un même réseau.",
      "Pour associer, lancez la « Détection automatique » et choisissez « Associer » dans « À proximité », ou saisissez une adresse dans « Ajouter un appareil », puis autorisez-le sur l’autre appareil. Autorisez-le seulement si les deux écrans affichent le même numéro de contrôle.",
      "Un NAS (SMB, FTP, NFS) s’ajoute de la même façon. Les appareils associés et les NAS servent de sources de jeux et de destinations ; leurs jeux n’y sont pas modifiés.",
      "« Transfert », dans le menu latéral, copie des fichiers et des dossiers entre eux. Un nom qui existe déjà à destination n’est remplacé qu’après votre confirmation.",
    ],
    helpSecurity: "Rien de ce qui est envoyé entre les appareils n’est chiffré : n’importe qui sur votre réseau peut le lire. Le mot de passe enregistré d’un NAS reste sur cet appareil, non chiffré. Utilisez Connexion uniquement sur votre réseau domestique.",
    helpSpeed: "« Test de débit », dans la ligne d’un appareil associé ou dans ses détails, lui envoie 128 MiB puis en récupère 128 MiB, sans rien écrire sur le disque, et affiche le débit du réseau dans chaque sens.",
    speedTest: "Test de débit",
    speedTestWith: (name) => `Test de débit avec ${name}`,
    speedTesting: "Test en cours",
    speedResult: (up, down) => `Envoi ${up} MB/s · Réception ${down} MB/s`,
    speedFailed: "Le test de débit a échoué :",
    speedNote: "Envoie 128 MiB puis en récupère 128 MiB ; rien n’est écrit sur le disque.",
    installTitle: "Dump Forge sur la PS5",
    ps5Address: "Adresse de la PS5",
    install: "Installer sur la PS5",
    installHint: "Nécessite le chargeur ELF de la console, accessible sur le réseau (port 9021).",
    findingRelease: "Recherche de la version sur GitHub",
    installing: (v, host) => `Installation de Dump Forge ${v} sur ${host}`,
    installed: (v, host) => `Dump Forge ${v} démarre sur ${host}. Associez-le depuis Ajouter un appareil une fois lancé.`,
    noRelease: "Impossible de trouver la version PS5 de Dump Forge sur GitHub. Vérifiez la connexion internet de cet ordinateur, puis réessayez.",
    otherPayload: "Tout autre payload ELF :",
    sendPayload: "Envoyer un payload",
    pickPayload: "Choisir un payload ELF",
    sendingPayload: (name, host) => `Envoi de ${name} à ${host}`,
    sentPayload: (name, size, host) => `${name} (${size}) envoyé à ${host}.`,
    helpInstallTitle: "Installer Dump Forge sur une PS5 en un clic",
    helpInstallText: "Utilisez la carte d’installation de Dump Forge pour envoyer le payload à la console. Elle télécharge depuis GitHub la version correspondante (ou la plus récente) et la lance.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "Installateur de paquets",
    packages: "Paquets",
    refresh: "Actualiser",
    scanning: "Recherche de paquets",
    none: "Aucun paquet trouvé sur cette console.",
    noneHint: "Copiez un fichier .pkg dans le stockage de la console ou sur une clé USB, puis appuyez sur Actualiser.",
    truncated: "La liste peut être incomplète",
    limits: "La recherche s’est arrêtée à l’une de ses limites : certains paquets peuvent manquer.",
    cantScan: "Impossible de rechercher :",
    cantInstall: "Impossible d’installer :",
    content: { game: "Jeu", patch: "Correctif", additional_content: "Contenu additionnel", unknown: "Inconnu" },
    install: "Installer",
    installName: (name) => `Installer ${name}`,
    started: (name: string) => `Installation de ${name}. La tâche apparaît dans « File d’attente ».`,
    tag: "Installation",
    title: "Installer ce paquet ?",
    titleOn: (device) => `Installer ce paquet sur ${device} ?`,
    file: "Fichier",
    game: "Jeu",
    titleId: "ID du titre",
    version: "Version",
    size: "Taille",
    text: "La PS5 récupère le paquet depuis Dump Forge et l’installe. Laissez Dump Forge ouvert jusqu’à la fin de la tâche.",
    reinstall: "Si ce jeu est déjà installé, la PS5 supprime la copie installée avant d’installer celle-ci.",
    cancel: "Annuler",
    confirm: "Installer",
    installPkg: "Installer un pkg",
    installPkgOn: (device) => `Installer un paquet sur ${device}`,
    pickTitle: "Choisir un paquet à installer",
    sent: (device: string) => `${device} installe le paquet. La tâche apparaît dans « File d’attente ».`,
  },
  // Build's and Edit's Restore metadata online switch (build/options.tsx, Edit.tsx)
  metadata: {
    tag: "Métadonnées",
    cant: "Impossible de restaurer les métadonnées :",
    label: "Restaurer les métadonnées en ligne",
    buildHint: "Avant la compilation, télécharge depuis le serveur de mises à jour de Sony le param.json, les images, le son et les tables PlayGo de cette version et les met dans le résultat là où ils diffèrent. Le jeu lui-même n’est pas modifié. Nécessite Internet ; sans lui la compilation continue telle quelle.",
    buildSummary: "Met les métadonnées de Sony dans le résultat (le jeu n’est pas modifié)",
    editHint: "Applique à ce jeu, avec vos modifications, le param.json, les images, le son et les tables PlayGo de cette version depuis le serveur de mises à jour de Sony là où ils diffèrent : un dossier est changé directement, un exfat ou ffpkg sur place, un ffpfs, ffpfsc ou FPKG reconstruit. Nécessite Internet ; si le téléchargement échoue, rien n’est changé.",
    editOnly: "Les métadonnées de Sony pour cette version, appliquées à ce jeu",
    editAlso: "Les métadonnées de Sony pour cette version sont aussi appliquées, téléchargées d’abord.",
    editConfirm: "Appliquer les métadonnées de Sony ?",
    editConfirmText: "Les métadonnées sont d’abord téléchargées ; si cela échoue, rien n’est changé.",
  },
  // App.tsx: the side rail
  rail: {
    build: "Créer",
    settings: "Réglages",
    queue: "File d’attente",
    transfer: "Transfert",
    collapse: "Réduire le menu",
    expand: "Développer le menu",
  },
  // Queue.tsx
  queue: {
    empty: "Aucune tâche pour l’instant. Les créations, modifications, transferts et installations apparaissent ici.",
    counts: (running: number, queued: number, failed: number, done: number): string =>
      [
        running && `${num(running)} en cours`,
        queued && `${num(queued)} en attente`,
        failed && `${num(failed)} en échec`,
        done && plural(done, { one: `${num(done)} terminée`, many: `${num(done)} terminées`, other: `${num(done)} terminées` }),
      ]
        .filter(Boolean)
        .join(", ") || "Rien à afficher",
    retryFailed: (n: number): string => (n > 0 ? `Relancer les échecs (${num(n)})` : "Relancer les échecs"),
    retrying: (i: number, n: number) => `Relance ${num(i)} sur ${num(n)}`,
    clearFinished: "Effacer les terminées",
    filters: { all: "Toutes", running: "En cours", failed: "En échec", done: "Terminées" },
    filterLabel: "Afficher",
    noneShown: "Aucune tâche ne correspond à ce filtre.",
  },
  // Settings.tsx
  settings: {
    appearance: "Apparence",
    theme: "Thème",
    themeHint: "Le sombre convient à une télé, le clair à une pièce lumineuse.",
    dark: "Sombre",
    light: "Clair",
    scheme: "Jeu de couleurs",
    schemeHint: "La couleur d’accent de toute l’application.",
    schemes: { abyss: "Abysse", ember: "Braise", moss: "Mousse", cobalt: "Cobalt", dusk: "Crépuscule", rose: "Rose", sand: "Sable", volt: "Volt", mono: "Mono" },
    advanced: "Options avancées",
    advancedOpen: "Toujours afficher les options avancées",
    advancedHint:
      "Affiche tous les paquets et formats, sans « Plus », et garde toutes les sections ouvertes. Désactivez-le pour que Créer reste court.",
    bundled: (version: string) => `${version} (fourni)`,
    replaceAmprEmu: "Remplacer ampr_emu par ampr-emu-ng",
    installedGames: "Afficher les jeux installés",
    installedGamesHint:
      "Affiche les jeux installés (/user/app) à côté de /data dans les sélecteurs et l'explorateur de fichiers, pour qu'un app.pkg de FPKG installé serve de source à Créer ou Modifier. Lecture seule : rien n'y est modifié ni supprimé. Un jeu du commerce ne peut pas être lu.",
    languageHint: "Les langues du système, jusqu’à ce que vous en choisissiez une.",
    notificationsGroup: "PS5",
  },
  // JobList.tsx: a finished job's result
  result: {
    file: "Sortie",
    type: "Type",
    size: "Taille",
    files: "Fichiers",
    time: "Durée",
    verified: "Vérifié",
    verifiedFast: "Vérifié (rapide)",
    verifiedFull: "Vérifié (complet)",
    notVerified: "Aucun contrôle signalé",
    checks: "Contrôles :",
    copy: "Copier les détails",
    copied: "Copié",
    copyFailed: "Copie impossible ici",
    ofSource: (pct: number) => `${num(pct, 1)} % des fichiers du jeu`,
    filesN: (n: number) => `${num(n)} ${plural(n, FILES)}`,
    to: "écrit dans",
    took: (d: string) => `en ${d}`,
    kinds: {
      pkg: "FPKG : s’installe sur une PS5 qui prend en charge les FPKG",
      exfat: "image exfat : ShadowMount+ la monte, en écriture",
      ffpkg: "image ffpkg (UFS2) : ShadowMount+ la monte, en écriture",
      ffpfs: "image ffpfs (PFS) : ShadowMount+ la monte",
      ffpfsc: "image ffpfsc (PFS compressé) : ShadowMount+ la monte en lecture seule",
      folder: "Dossier de jeu",
    } as Record<string, string | undefined>,
    zstdFolder: "Dossier empaqueté en ZSTD",
    lz4Folder: "Dossier empaqueté en LZ4",
    withPacks: (image: string, kind: string) => `${image}, fichiers dans des paquets ${kind}`,
    edited: (what: string) => `${what}, modifié sur place`,
    rebuilt: (what: string) => `${what}, reconstruit`,
    extracted: "Fichiers extraits",
    transferred: "Fichiers copiés",
    installed: "Paquet transmis à l’installateur de la PS5",
  },
  // JobList.tsx: the log panel
  log: {
    lines: (n: number, of?: number): string =>
      of === undefined
        ? `${num(n)} ${plural(n, { one: "ligne", many: "de lignes", other: "lignes" })}`
        : `${num(n)} ${plural(n, { one: "ligne", many: "de lignes", other: "lignes" })} sur ${num(of)}`,
    filter: "Filtrer",
    follow: "Défilement auto",
    copy: "Copier",
    copied: "Copié",
    copyFailed: "Échec de la copie",
    save: "Enregistrer",
    clear: "Effacer",
    cleared: "Effacé",
  },
  // common.tsx: the source card's fact chips
  sourceChips: {
    sdk: (v: string) => `SDK ${v}`,
    firmware: (v: string) => `FW ${v}+`,
  },
  // prefs.tsx: Advanced's badge
  advanced: {
    changed: "Modifiées",
    defaults: "Par défaut",
  },
  // build/*: the Build screen
  build: {
    summary: "Résumé",
    addToQueue: "Ajouter à la file",
    queued: "Ajouté à la file d’attente",
    openQueue: "Ouvrir la file",
    space: (free: string, dir: string, needed: string) => (
      <>
        {free} libres dans <span className="path">{dir}</span>, environ {needed} nécessaires
      </>
    ),
    needed: (needed: string) => `Environ ${needed} nécessaires`,
    spaceRaw: "L'espace est vérifié comme si rien n'était compressé.",
    estimate: "estimation",
    advanced: "Options avancées",
    options: "Options",
    fastVerify: "Vérification rapide : chaque structure est contrôlée, un échantillon des données est relu.",
    fullVerify: "Vérification complète : chaque octet est relu.",
    deleted: "La source est supprimée après une création réussie. Irréversible.",
    zstdBackport:
      "Les ressources de ce jeu sont empaquetées en ZSTD et son index ZSTD inclut le backport : créez-le d’abord une fois avec Paquets : Aucun, puis retirez le backport.",
    lz4Backport:
      "Les ressources de ce jeu sont empaquetées en LZ4 et son manifeste LZ4 inclut le backport : créez-le d’abord une fois avec Paquets : Aucun, puis retirez le backport.",
    backportLeftOut: (n: number) =>
      plural(n, {
        one: `${num(n)} bibliothèque de backport est laissée hors de fakelib/.`,
        many: `${num(n)} de bibliothèques de backport sont laissées hors de fakelib/.`,
        other: `${num(n)} bibliothèques de backport sont laissées hors de fakelib/.`,
      }),
    packsTitle: "Cible",
    packs: "Paquets",
    more: "Plus",
    moreTip: "Plus de formats",
    cards: {
      folder: "Fichiers ordinaires, à copier n’importe où, ShadowMount+ le lance",
      pkg: "S’installe sur la PS5 comme un paquet de jeu",
      exfat: "Se monte avec ShadowMount+ et s’ouvre sur un PC",
      ffpkg: "Image UFS2 pour ShadowMount+, montage en écriture possible",
      ffpfs: "Image PFS, non compressée, noms de fichiers ASCII uniquement",
      ffpfsc: "L’image la plus petite : compressée, toujours montée en lecture seule",
      lz4: "Ressources empaquetées en LZ4, dossier ordinaire",
    },
    zstdFolderCard: "Ressources empaquetées en ZSTD, dossier ordinaire",
    packCards: {
      nonePlain: "Les fichiers du jeu tels quels",
      unpack: "Réécrit les paquets sous forme de fichiers ordinaires",
      keepZstd: "Les paquets ZSTD sont gardés octet pour octet",
      keepLz4: "Les paquets LZ4 sont gardés octet pour octet",
      zstd: "Jeu plus petit, chargement plus rapide, avec ampr-emu-ng",
      repackZstd: "Décode les paquets LZ4, empaquette à nouveau en ZSTD",
      lz4: "Empaquette les ressources en LZ4, avec ampr-emu-ng",
      repackLz4: "Décode les paquets ZSTD, empaquette à nouveau en LZ4",
    },
    naming: "Sortie : nom en cours de création",
    output: (name: string, dir: string) => (
      <>
        Sortie : <b className="path">{name}</b> dans <span className="path">{dir}</span>
      </>
    ),
    choices: { none: "Aucun", zstd: "ZSTD", lz4: "LZ4" },
    levelLabel: (level: string, engine: string, n: number) => `${level}, ${engine} ${n}`,
    levelTip: (engine: string, n: number, note: string) => `${engine} ${n}. ${note}`,
    sumKeepZstd: "Les paquets ZSTD sont gardés octet pour octet.",
    sumKeepLz4: "Les paquets LZ4 sont gardés octet pour octet.",
    sumKeepPlain: "Aucun paquet n’est créé : les fichiers du jeu sont convertis tels quels.",
    sumUnpack: (engine: string) => `Les paquets ${engine} sont réécrits sous forme de fichiers ordinaires.`,
    sumPack: (engine: string, level: string) => `Les fichiers du jeu sont empaquetés dans des volumes ${engine} (${level}).`,
    sumRepack: (from: string, to: string, level: string) => `Les paquets ${from} sont décodés puis empaquetés à nouveau en ${to} (${level}).`,
    sumRules: (name: string) => `Les règles personnalisées ${name} choisissent les fichiers à empaqueter.`,
    sumInside: (format: string, level: number) => `À l’intérieur : ${format}, niveau zlib ${level}.`,
    ngInstalled: "ampr-emu-ng est installé dans fakelib/libSceAmpr.sprx.",
    ngReplaced: (old: string) => `L’${old} du jeu est remplacé par ampr-emu-ng.`,
    ngReplacedTrace: "L’ancien runtime de trace du jeu est remplacé par ampr-emu-ng.",
    ngKept: (old: string) => `L’${old} du jeu est gardé.`,
    ngUpdated: (v: string) => `L’ampr-emu-ng${v} du jeu est remplacé par celui fourni.`,
    ngSame: (v: string) => `L’ampr-emu-ng${v} du jeu est gardé.`,
    pkgNoRuntime: "Le runtime AMPR est laissé hors du FPKG : le chargeur de la console le fournit.",
    recommended: "Recommandé",
    resultTitle: "Environ, après la création",
    resultNow: "Jeu actuel",
    resultCustomRules: "règles personnalisées : pas d’estimation",
    backportTitle: "Backport",
    backportNone: "Ce jeu n’a pas de backport.",
    backportPlain: "Uniquement pour une conversion simple : sans empaquetage ni dépaquetage.",
  },
  // TransferPane.tsx: one device's pane of the Transfer screen
  transfer: {
    pane: (device) => `Fichiers sur ${device}`,
    roots: "Lecteurs",
    location: "Emplacement",
    up: "Remonter",
    upTip: "Remonter d’un dossier",
    refresh: "Actualiser",
    filter: "Filtrer par nom",
    filterLabel: "Filtrer ce dossier par nom",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "dossier", many: "de dossiers", other: "dossiers" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) =>
      plural(n, { one: `${num(n)} correspondance`, many: `${num(n)} de correspondances`, other: `${num(n)} correspondances` }),
    firstOf: (max, n) => `${num(max)} premiers sur ${num(n)}`,
    truncated: "Ce dossier contient plus de 10 000 éléments : seuls les 10 000 premiers sont listés.",
    name: "Nom",
    size: "Taille",
    select: (name) => `Sélectionner ${name}`,
    open: (name) => `Ouvrir ${name}`,
    selected: (n) =>
      plural(n, { one: `${num(n)} élément sélectionné`, many: `${num(n)} d’éléments sélectionnés`, other: `${num(n)} éléments sélectionnés` }),
    clear: "Effacer",
    clearTip: "Effacer la sélection",
    dragItems: (n) => plural(n, { one: `${num(n)} élément`, many: `${num(n)} d’éléments`, other: `${num(n)} éléments` }),
    dropHere: (path) => `Déposer pour envoyer vers ${path}`,
    notHere: "Ouvrez un dossier pour y envoyer des fichiers.",
    emptyFolder: "Ce dossier est vide.",
    noMatch: (f) => `Rien ici ne correspond à « ${f} ».`,
    noRoots: "Aucun lecteur n’est disponible.",
    cantList: "Impossible d’ouvrir ce dossier :",
    loading: "Chargement",
    link: "Lien",
    linkTip: "Un lien : il peut être ouvert, mais pas sélectionné",
    mounted: "Monté",
    mountedTip: "Monté par ShadowMount+ : quittez le jeu et désactivez ShadowMount+ pour le sélectionner",
    withTitle: "Transférer avec",
    loadingDevices: "Chargement des appareils",
    noneTitle: "Aucun appareil avec qui transférer",
    noneHint: "Associez un autre Dump Forge ou ajoutez un NAS dans « Connexion », ou allumez un appareil enregistré.",
    openConnect: "Ouvrir « Connexion »",
    deviceOffline: (device) => `${device} est hors ligne. Ses fichiers réapparaîtront dès son retour.`,
    copyTo: (n, device) =>
      plural(n, {
        one: `Copier ${num(n)} élément vers ${device}`,
        many: `Copier ${num(n)} d’éléments vers ${device}`,
        other: `Copier ${num(n)} éléments vers ${device}`,
      }),
    copyNoFolder: (device) => `Ouvrez un dossier sur ${device} où copier.`,
    installOn: (device) => `Installer sur ${device}`,
    dropFiles: "Déposez des fichiers ici pour les envoyer",
    uploadInto: (folder) => `Ils vont dans ${folder}, le dossier affiché sur cet appareil.`,
    uploadNoFolder: "Ouvrez un dossier sur cet appareil où envoyer les fichiers.",
    foldersLeftOut: (names) => `Les dossiers ne sont pas envoyés, seulement les fichiers : ${names}.`,
    downloadFiles: (n) =>
      plural(n, {
        one: `Télécharger ${num(n)} fichier`,
        many: `Télécharger ${num(n)} de fichiers`,
        other: `Télécharger ${num(n)} fichiers`,
      }),
    downloadNone: "Sélectionnez des fichiers sur cet appareil pour les télécharger (les dossiers ne sont pas téléchargés).",
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default fr;
