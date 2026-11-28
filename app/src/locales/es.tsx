// Spanish (neutral). Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Terms: traza/trazas (the recorded traces), trazado (tracing), runtime, empaquetar/desempaquetar,
// parchear/quitar parche.

import { dotted, num, plural } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "Igual al tamaño del juego";
const FILES = { one: "archivo", many: "archivos", other: "archivos" };
const CHECKS = { one: "comprobación", many: "comprobaciones", other: "comprobaciones" };

const es: Messages = {
  // App.tsx
  app: {
    screens: "Pantallas",
    active: (n) => `, ${num(n)} ${plural(n, { one: "activa", many: "activas", other: "activas" })}`,
    edit: "Editar",
    storage: "Almacenamiento",
    connect: "Conectar",
    disconnected: "Desconectado",
    connected: (n: number) => plural(n, { one: `${num(n)} dispositivo`, many: `${num(n)} dispositivos`, other: `${num(n)} dispositivos` }),
    about: "Acerca de",
    offline: "Sin conexión, reintentando…",
    stop: "Detener",
    stillRunning: (n) =>
      n === 1
        ? "Una tarea sigue en curso"
        : `${num(n)} ${plural(n, { one: "tarea sigue en curso", many: "tareas siguen en curso", other: "tareas siguen en curso" })}`,
    job: (id) => `Tarea ${id}`,
    quitNote: (web, n) =>
      web
        ? n === 1
          ? "Al detener Dump Forge, la tarea se detiene y se elimina la salida sin terminar."
          : "Al detener Dump Forge, las tareas se detienen y se elimina la salida sin terminar."
        : n === 1
          ? "Al salir, la tarea se detiene y se elimina la salida sin terminar."
          : "Al salir, las tareas se detienen y se elimina la salida sin terminar.",
    keepRunning: "Seguir",
    stopAndQuit: (n) => (n === 1 ? "Detener tarea y salir" : "Detener tareas y salir"),
    stoppingJobs: "Deteniendo tareas y limpiando…",
    stoppingForge: "Deteniendo Dump Forge…",
    quitting: "Saliendo de PS5 Dump Forge…",
    couldntStop: "No se pudo detener Dump Forge",
    close: "Cerrar",
    tryAgain: "Reintentar",
    stopped: "Dump Forge se ha detenido",
    startAgain: (ps5) => (ps5 ? "Para continuar, vuelve a abrir el icono de Dump Forge en la PS5." : "Para continuar, vuelve a ejecutar el mismo comando ps5-dump-forge serve."),
    confirmStop: (web: boolean) => (web ? "¿Detener Dump Forge?" : "¿Salir de PS5 Dump Forge?"),
    stopNote: (web, ps5) => (!web ? "No hay ningún trabajo en curso." : ps5 ? "Para volver a iniciarlo, abre el icono de Dump Forge en la PS5." : "Para volver a iniciarlo, ejecuta de nuevo el mismo comando ps5-dump-forge serve."),
    peerBusy: "Un dispositivo vinculado está leyendo o escribiendo archivos en este equipo. Al salir, su transferencia se detiene.",
    quit: "Salir",
    github: (name) => `${name} en GitHub`,
    queued: "en cola",
    progress: (percent, left) => `${percent}% completado${left ? `, tiempo restante: ${left}` : ""}`,
  },
  // build/ (Build's convert settings and checks)
  convert: {
    targetFormat: "Formato",
    alreadyFolder: "el origen ya es una carpeta",
    patchBlocked: "No se puede crear: es un paquete de parche, que solo contiene lo que cambió la actualización. Ábrelo en “Editar” para extraer sus archivos.",
    openInEdit: "Abrir en “Editar”",
    noContentId: "No se puede crear como FPKG: el param.json de este juego no tiene Content ID, y un paquete lo necesita.",
    blocked: (first, more) =>
      `No se puede crear: ${first}${more > 0 ? ` (y ${num(more)} más, consulta “Origen”)` : ""}.`,
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index no recoge los archivos de este juego" : state === "damaged" ? "No se puede leer ampr_emu.index" : "Este juego no tiene ampr_emu.index"}: la conversión escribe uno nuevo.`,
    removeBackport: "Quitar backport",
    removeHint: (libs, emulators, other) =>
      `Omite ${num(libs)} ${plural(libs, { one: "biblioteca", many: "bibliotecas", other: "bibliotecas" })} de backport de fakelib/` +
      (emulators.length === 0
        ? other
          ? "; las demás bibliotecas homebrew se conservan."
          : "."
        : other
          ? emulators.length === 1
            ? `; el emulador (${emulators.join(", ")}) y las demás bibliotecas homebrew se conservan.`
            : `; los emuladores (${emulators.join(", ")}) y las demás bibliotecas homebrew se conservan.`
          : emulators.length === 1
            ? `; el emulador (${emulators.join(", ")}) se conserva.`
            : `; los emuladores (${emulators.join(", ")}) se conservan.`),
    cantRemove: (why) => `No se puede quitar el backport: ${why}`,
    compare: "Comparar formatos",
    build: "Crear",
    likelyFail: (fmt) => (
      <>
        Probablemente fallará como <b>{fmt}</b>: no admite los nombres de archivo indicados en Origen.
      </>
    ),
    collides: "Una tarea ya está escribiendo este archivo: volver a crear fallaría. Elige otro nombre.",
    already: (fmt) => (
      <>
        Este juego ya se está creando como <b>{fmt}</b>; volver a crear hace una segunda copia.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "Bueno", ok: "Compromiso", bad: "Desventaja" },
    columns: ["Velocidad en consola", "Tamaño en disco"],
    rows: {
      pkg: {
        tagline: "El mejor, si tu firmware lo permite",
        cells: ["Nativa, velocidad completa", "Entre 30 y 60% menos, según el juego"],
      },
      zstd: { tagline: "Con ampr-emu-ng", cells: ["Carga más rápida", "Hasta la mitad del tamaño del juego"] },
      lz4: {
        tagline: "Con ampr-emu-ng",
        cells: ["Carga entre 30 y 40% más rápida, según sus autores", "Más o menos la mitad del juego, según sus autores"],
      },
      ffpkg: { tagline: "Recomendado para ShadowMountPlus", cells: ["Velocidad completa de imagen", FLAT] },
      exfat: { tagline: "Compatibilidad", cells: ["Velocidad completa de imagen", FLAT] },
      ffpfs: { tagline: "PFS sin comprimir", cells: ["Velocidad completa de imagen", FLAT] },
      ffpfsc: {
        tagline: "El más pequeño",
        cells: ["150–250 MB/s, puede dar tirones", "Depende del nivel de compresión, normalmente entre 30 y 60% menos"],
      },
      folder: { tagline: "Archivos del juego sin más", cells: ["Velocidad completa de la unidad", FLAT] },
    },
    compare: "Comparar formatos",
    compareLabel: "Comparación de formatos",
    format: "Formato",
    credits: "Créditos",
    upstream: (app: string) =>
      `Cifras de sus autores: lo que indican los autores de ampr_emu y Lazy_AMPR, no medido por ${app}; varían según el juego.`,
    bundled: "ampr_emu de drakmor (GPL-3.0), del que deriva ampr-emu-ng. Forge ya no lo incluye: sustituye el ampr_emu de un juego por ampr-emu-ng, salvo que Ajustes lo conserve. Código fuente:",
    lazy: "La estrategia de empaquetado se basa en Lazy_AMPR de Nazky:",
    ng: "Los paquetes ZSTD y LZ4 funcionan con ampr-emu-ng (GPL-3.0-or-later), el único runtime de AMPR incluido, sin modificar. Código fuente:",
    logo: "El logotipo es obra de Xxcco:",
    unrar:
      "Los archivos .rar se leen con la biblioteca UnRAR incluida, de Alexander Roshal (solo extracción, bajo la licencia de UnRAR).",
    language: "Idioma",
    system: "Sistema",
    notifications: "Avisar al terminar una tarea",
    notificationsHint: "Se muestra en la consola al terminar una tarea de creación, empaquetado o transferencia.",
    notificationsFailed: "No se pudieron cambiar las notificaciones.",
  },
  // Edit.tsx
  edit: {
    details: "Detalles",
    deleting: "Eliminando el origen",
    delete: "Eliminar el origen",
    deleted: "Eliminado:",
    filter: "Filtrar por ruta",
    filterLabel: "Filtrar archivos por ruta",
    firstOf: (max, n) => plural(max, {
  one: `primero de ${num(n)}`,
  many: `primeros ${num(max)} de ${num(n)}`,
  other: `primeros ${num(max)} de ${num(n)}`,
}),
    fileCount: (n) => `${num(n)} ${plural(n, FILES)}`,
    noMatch: (f) => `Ningún archivo coincide con “${f}”.`,
    noListing: "La lista de archivos del archivo comprimido no es compatible.",
    noParam: "No hay sce_sys/param.json.",
    reader: "Lector",
    leftovers: "Archivos y carpetas sobrantes",
    found: (n) => `${num(n)} ${plural(n, { one: "encontrado", many: "encontrados", other: "encontrados" })}`,
    leftoversNote: (app) =>
      `Salidas sin terminar, normalmente de tareas interrumpidas. Elimina solo las que reconozcas, y solo cuando no haya ninguna tarea en curso; ${app} nunca las elimina por su cuenta.`,
    checked: (n) => `Carpetas revisadas (${num(n)})`,
    facts: "Datos del juego",
    explorer: "Archivos del juego",
    howTitle: "Cómo funciona",
    how: (app) => [
      "Abre una **carpeta** o **imagen** de un juego y cámbiala como una carpeta en una memoria USB: **añade y reemplaza archivos** (arrástralos o usa “Añadir”), **cambia nombres y elimina**.",
      `Nada cambia **hasta que pulses “Aplicar”**: tus cambios se acumulan aquí y luego ${app} los escribe de una vez directamente en la imagen o la carpeta y comprueba el resultado. **No es una unidad montada**: ${app} lleva la cuenta de tus cambios y los aplica por sí mismo.`,
      `Si los cambios caben, van **directamente a la imagen**; si no (no queda espacio dentro o el formato no los admite), ${app} crea al lado una imagen nueva con tus cambios y la pone en su lugar cuando supera la verificación. En ambos casos, si algo falla, la imagen queda como estaba.`,
      "Los archivos comprimidos se pueden explorar, pero no editar.",
    ],
    location: "Ubicación en el juego",
    root: "Juego",
    up: "Subir",
    upTip: "Subir una carpeta",
    name: "Nombre",
    size: "Tamaño",
    actions: "Acciones",
    items: (n) => `${num(n)} ${plural(n, { one: "elemento", other: "elementos" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "carpeta", other: "carpetas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "coincidencia", other: "coincidencias" })}`,
    emptyFolder: "Esta carpeta está vacía.",
    open: (name) => `Abrir ${name}`,
    archiveNote: "Los archivos comprimidos se pueden explorar, pero no editar. Para cambiarlo, crea antes una carpeta o una imagen a partir de él.",
    rebuildTimePkg: (t) => `Reconstruir vuelve a comprimir todo el juego: unos ${t} a ~26 MB/s en una PS5 (más rápido en un ordenador). Edita una copia desempaquetada para evitarlo.`,
    rebuildTimePs5: (t) => `Reconstruir vuelve a escribir toda la imagen (unos ${t} en esta PS5).`,
    rebuildTimeHost: (t) => `Reconstruir vuelve a escribir toda la imagen (unos ${t} en este ordenador).`,
    rebuildTimeAny: (t) => `Reconstruir vuelve a escribir toda la imagen (unos ${t} en una PS5).`,
    nasNote: "Los juegos de un NAS se pueden explorar y extraer, pero no editar. Para cambiarlo, créalo o extráelo primero en este dispositivo.",
    patchNote: "Es un paquete de parche: solo contiene lo que cambió la actualización. Se muestran los archivos que contiene completos; se pueden explorar y extraer, pero no editar.",
    peerNote: "Los juegos de otro Forge se pueden explorar y extraer, pero no editar. Para cambiarlo, créalo o extráelo primero en este dispositivo.",
    addFiles: "Añadir archivos",
    addFolder: "Añadir carpeta",
    pickAddFiles: "Elige los archivos que quieres añadir",
    pickAddFolder: "Elige la carpeta que quieres añadir",
    peerRoot: "No se puede añadir un dispositivo entero: elige una carpeta dentro de él.",
    renameItem: (name) => `Cambiar el nombre de ${name}`,
    removeItem: (name) => `Eliminar ${name}`,
    renameTip: "Cambiar nombre (F2)",
    removeTip: "Eliminar (tecla Supr)",
    removeTipMac: "Eliminar (teclas fn+Eliminar)",
    extractItem: (name: string) => `Extraer ${name}`,
    extractTip: "Copiar a una carpeta que elijas",
    extractFolder: "Extraer carpeta",
    extractFolderTip: "Copiar la carpeta mostrada a una carpeta que elijas",
    extractGameTip: "Copiar el juego completo a una carpeta que elijas",
    extractNotOnDisk: "Esta carpeta solo existe en tus cambios pendientes: aplícalos para extraerla.",
    pickExtract: "Elige dónde extraer",
    extractPending: "Extrae los archivos tal como están ahora, sin tus cambios pendientes.",
    nameEmpty: "Escribe un nombre.",
    nameBad: "Un nombre no puede contener / ni \\, ni ser . o ..",
    nameTaken: (name) => `${name} ya existe aquí.`,
    markNew: "Nuevo",
    markReplaced: "Reemplazado",
    markRenamed: "Renombrado",
    wasAt: (path) => `Antes: ${path}`,
    sizeLater: "Más de lo que se pudo listar: el resto se lee al aplicar los cambios",
    dropInto: (folder) => `Suelta para añadir a ${folder}`,
    droppedGone: (path) => `${path}: el archivo arrastrado ya no está`,
    pending: (n) =>
      plural(n, { one: `${num(n)} cambio sin aplicar todavía`, other: `${num(n)} cambios sin aplicar todavía` }),
    undo: "Deshacer",
    undoTip: "Deshacer la última acción, con todos sus cambios",
    discard: "Descartar todo",
    discardTip: "Olvidar todos los cambios aún sin aplicar",
    discardTitle: (n) =>
      plural(n, { one: `¿Descartar ${num(n)} cambio?`, other: `¿Descartar los ${num(n)} cambios?` }),
    discardText: "Se olvidan y nada cambia en el juego. Esto no se puede deshacer.",
    discardNo: "No",
    changesList: "Cambios, en orden",
    willNotApply: (n) =>
      plural(n, {
        one: `${num(n)} cambio no se puede aplicar tal como está. Deshazlo o descarta todo.`,
        other: `${num(n)} cambios no se pueden aplicar tal como están. Deshazlos o descarta todo.`,
      }),
    opAdd: (to) => <>Añadir {to}</>,
    opAddFolder: (to) => <>Añadir la carpeta {to}</>,
    opDelete: (path) => <>Eliminar {path}</>,
    opRename: (from, to) => <>Renombrar {from} a {to}</>,
    opMkdir: (path) => <>Nueva carpeta {path}</>,
    opFrom: (src) => <>Desde {src}</>,
    whyInvalid: (path) => <>{path} no es una ruta válida del juego.</>,
    whyMissing: (path) => <>{path} no está en el juego.</>,
    whyExists: (path) => <>{path} ya existe.</>,
    whyFile: (path) => <>{path} es un archivo.</>,
    whyFolder: (path) => <>{path} es una carpeta.</>,
    whyInto: (path) => <>{path} no se puede mover dentro de sí mismo.</>,
    conflictTitle: (n) =>
      plural(n, { one: `${num(n)} elemento ya existe aquí`, other: `${num(n)} elementos ya existen aquí` }),
    conflictIn: (folder) => <>En {folder}:</>,
    conflictText:
      "“Reemplazar” pone los nuevos en su lugar: una vez aplicados los cambios, el contenido anterior desaparece para siempre, y un archivo defectuoso puede impedir que el juego arranque. Una carpeta añadida sobre otra carpeta se combina con ella y sus archivos con el mismo nombre se reemplazan.",
    replace: "Reemplazar",
    skip: "Omitir",
    cancel: "Cancelar",
    modeLabel: "Cómo aplicar",
    checking: "Comprobando cómo se pueden aplicar los cambios",
    nothingToChange: "Nada que cambiar: el juego queda como está.",
    checkFailed: "No se pudieron comprobar los cambios:",
    checkAgain: "Volver a comprobar",
    inPlace: "En el sitio",
    rebuild: "Reconstruir",
    rebuildNeeded: "Hay que reconstruir",
    cantApply: "No se puede aplicar",
    changedDirectly: "Se cambia directamente",
    folderText: "Se escribe directamente en la carpeta.",
    inPlaceText: "Se escribe en la imagen.",
    inPlaceNeeds: (need: string, free: string) => `Se escribe en la imagen; necesita ${need} de sus ${free} libres.`,
    noRoom: (need: string, free: string) => `Los cambios necesitan ${need} y la imagen tiene ${free} libres.`,
    alwaysRebuilt: "Las imágenes ffpfsc y FPKG siempre se reconstruyen.",
    rebuildText: "Se escribe una imagen nueva al lado, que la reemplaza una vez verificada.",
    rebuildNeeds: (need: string, free: string) =>
      `Se escribe una imagen nueva al lado (hacen falta unos ${need} libres, hay ${free} libres), que la reemplaza una vez verificada.`,
    uploadLater: (n: number) =>
      plural(n, {
        one: `${num(n)} archivo de este navegador se comprueba cuando se sube, al pulsar “Aplicar”.`,
        other: `${num(n)} archivos de este navegador se comprueban cuando se suben, al pulsar “Aplicar”.`,
      }),
    decidedOnApply: (n: number) =>
      plural(n, {
        one: `Otros cambios dependen del archivo de este navegador, así que la forma de aplicarlos se decide cuando se sube, al pulsar “Aplicar”.`,
        other: `Otros cambios dependen de los ${num(n)} archivos de este navegador, así que la forma de aplicarlos se decide cuando se suben, al pulsar “Aplicar”.`,
      }),
    sourceLocked: "Se están aplicando cambios a este juego. Elige otro origen cuando termine la tarea.",
    notOpened: (path) => <>{path} no se abrió: se están aplicando cambios a este juego. Vuelve a abrirlo cuando termine la tarea.</>,
    apply: "Aplicar",
    applyTip: "Aplicar todos los cambios de esta lista",
    applyTitle: (n: number) => plural(n, { one: `¿Aplicar ${num(n)} cambio?`, other: `¿Aplicar ${num(n)} cambios?` }),
    applyInPlace: (name) => <>Se escriben directamente en {name}. Una vez hecho, no se puede deshacer.</>,
    applyRebuild: (name) => (
      <>Se escribe al lado de {name} una imagen nueva con tus cambios, se verifica y luego ocupa su lugar. Una vez que reemplaza la imagen, no se puede deshacer.</>
    ),
    closeImage: "Si el juego se está ejecutando desde esta imagen, ciérralo primero.",
    closeFolder: "Si el juego se está ejecutando desde esta carpeta, ciérralo primero.",
    applyRestores: "Una edición anterior de esta imagen se interrumpió: primero se deja la imagen como estaba.",
    uploadingFile: (i: number, n: number) => `Subiendo el archivo ${num(i)} de ${num(n)}`,
    uploading: (i: number, n: number, sent: string, total: string) => `Subiendo el archivo ${num(i)} de ${num(n)}: ${sent} de ${total}`,
    uploadLabel: "Subida",
    uploadCancelled: "Subida cancelada. No cambió nada en el juego.",
    applyRunning: "Aplicando los cambios. La edición espera a que termine la tarea (ver “Cola”).",
    applyFailed: "No se aplicaron los cambios:",
    applyRefused: "Los cambios no se pueden aplicar de esta forma:",
    interrupted: "Una edición anterior de esta imagen se interrumpió. La imagen se deja como estaba antes de comprobar o aplicar tus próximos cambios.",
  },
  // JobList.tsx
  jobs: {
    jobs: "Tareas",
    pending: (n) => `${num(n)} en curso o en cola`,
    allFinished: "todas terminadas",
    stages: {
      scan: "Leyendo el origen",
      preflight: "Comprobando",
      check: "Comprobando",
      plan: "Planificando",
      compress: "Comprimiendo",
      write: "Escribiendo",
      measure: "Midiendo paquetes",
      pack: "Empaquetando",
      verify: "Verificando",
      finalize: "Finalizando",
      install: "Instalando",
    },
    steps: {
      scan: "Lectura",
      preflight: "Comprobación",
      check: "Comprobación",
      plan: "Planificación",
      compress: "Compresión",
      write: "Escritura",
      measure: "Medición",
      pack: "Empaquetado",
      verify: "Verificación",
      finalize: "Finalización",
      install: "Instalación",
    },
    stepsLabel: "Pasos",
    stepDone: (s) => `Hecho: ${s}`,
    stepFailed: (s) => `Falló: ${s}`,
    stepLater: (s) => `Pendiente: ${s}`,
    packingWriting: "Empaquetando y escribiendo",
    rate: { read: "lectura", compress: "compresión", pack: "empaquetado", write: "escritura" },
    showPath: "Mostrar ruta",
    showFinder: "Mostrar en Finder",
    showExplorer: "Mostrar en el Explorador",
    showFolder: "Mostrar en la carpeta",
    packTag: {
      lz4Pack: (level: string) => `LZ4: empaquetar · ${level}`,
      lz4Unpack: "LZ4: desempaquetar",
      zstdPack: (level: string) => `ZSTD: empaquetar · ${level}`,
      level: (n: number) => `nivel ${num(n)}`,
      zstdUnpack: "ZSTD: desempaquetar",
      withRules: (tag) => `${tag} · reglas propias`,
    },
    edit: "Edición",
    editRebuild: "Edición · reconstrucción",
    transfer: "Transferencia",
    extract: "Extracción",
    job: (id) => `Tarea ${id}`,
    done: "Hecho",
    cancelled: "Cancelada",
    failed: "Error",
    cancelling: "Cancelando…",
    queued: "En cola",
    dismiss: "Descartar",
    retry: "Reintentar",
    retrying: "Iniciando",
    stopJob: "¿Detener tarea?",
    cancel: "Cancelar",
    stopHint: "Vuelve a pulsar “¿Detener tarea?” en menos de 4 segundos para detenerla.",
    remoteLabel: (name, device) => `${name}, una tarea en ${device}`,
    remoteTitle: (device) => `Se ejecuta en ${device} y usa archivos de este dispositivo: aquí solo puedes cancelarla`,
    from: "de ",
    left: (d) => `tiempo restante: ${d}`,
    inStep: (time) => `${time} en este paso`,
    outputPath: "Ruta de salida",
    sourceDeleted: (v) => `Origen eliminado: ${v}`,
    sourceKept: (v) => `Origen conservado: ${v}`,
    sourcePartlyDeleted: (v) => `Origen eliminado en parte: ${v}`,
    checks: "Comprobaciones",
    verifiedPlain: (n) => `Verificado: ${num(n)} ${plural(n, CHECKS)} sin errores`,
    verifiedFull: (n) => `Verificación completa superada: ${num(n)} ${plural(n, CHECKS)}`,
    verifiedFast: (n) => `Verificación rápida superada: ${num(n)} ${plural(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `Cantidad revisada: ${checked} de ${total} en ${num(n)} ${plural(n, { one: "muestra", many: "muestras", other: "muestras" })}`,
    cantBuild: "No se puede crear este juego:",
    cantEdit: "No se pueden aplicar los cambios:",
    cantExtract: "No se puede extraer:",
    cantTransfer: "No se puede transferir:",
    cantBuildAs: (f) => (
      <>
        No se puede crear este juego como <b>{f}</b>:
      </>
    ),
    log: (n) => `Registro (${num(n)})`,
    logLabel: "Registro",
  },
  // Storage text shared by Files.tsx, Packages.tsx, Edit.tsx and common.tsx (FreeSpace)
  library: {
    cantDelete: "No se puede eliminar:",
    deleted: (path) => `Eliminado ${path}`,
    storageTip: "Espacio libre en el almacenamiento interno de la consola (/data)",
    free: (free, total) => (
      <>
        Libre: <b>{free}</b> de {total}
      </>
    ),
    cards: "Vista de tarjetas",
    list: "Vista de lista",
    remove: (n) => `Eliminar ${n}`,
    deleting: (n) => `Eliminando ${n}`,
    showAs: "Mostrar como",
  },
  // Storage.tsx, Files.tsx: the Storage tab (File explorer | Package installer)
  storage: {
    views: "Vista de almacenamiento",
    files: "Explorador de archivos",
    title: "Explorador de archivos",
    roots: "Unidades",
    location: "Ubicación",
    folders: "Carpetas",
    up: "Subir",
    upTip: "Subir una carpeta",
    refresh: "Actualizar",
    filter: "Filtrar por nombre",
    filterLabel: "Filtrar esta carpeta por nombre",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "carpeta", other: "carpetas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "coincidencia", other: "coincidencias" })}`,
    firstOf: (max, n) => `primeros ${num(max)} de ${num(n)}`,
    truncated: "Esta carpeta contiene más de 10.000 elementos: solo se muestran los primeros 10.000.",
    name: "Nombre",
    size: "Tamaño",
    emptyFolder: "Esta carpeta está vacía.",
    noMatch: (f) => `Nada de aquí coincide con “${f}”.`,
    noRoots: "No hay unidades disponibles para explorar.",
    cantList: "No se puede abrir esta carpeta:",
    link: "Enlace",
    linkTip: "Un enlace: se puede abrir, pero aquí no se puede cambiar de nombre, mover ni eliminar",
    mounted: "Montado",
    mountedTip: "Montado por ShadowMount+: detén el juego y desactiva ShadowMount+ para eliminarlo, cambiarle el nombre o moverlo",
    mountedNote: "ShadowMount+ está usando las imágenes marcadas como Montado: detén el juego y desactiva ShadowMount+ para eliminarlas, cambiarles el nombre o moverlas.",
    open: (n) => `Abrir ${n}`,
    more: (n) => `Más acciones para ${n}`,
    rename: (n) => `Cambiar el nombre de ${n}`,
    renameTip: "Cambiar nombre (F2)",
    move: (n) => `Mover ${n}`,
    moveTip: "Mover a otra carpeta",
    moveTitle: (n) => `Mover ${n} a`,
    remove: (n) => `Eliminar ${n}`,
    removeTip: "Eliminar (tecla Supr)",
    removeTipMac: "Eliminar (teclas fn+Eliminar)",
    cantDelete: "No se puede eliminar:",
    cantRename: "No se puede cambiar el nombre:",
    cantMove: "No se puede mover:",
    sameFolder: "Ya está en esa carpeta.",
    deleted: (path) => `Eliminado ${path}`,
    renamed: (path) => `Nombre cambiado a ${path}`,
    moved: (path) => `Movido a ${path}`,
    devices: "Dispositivos",
    thisDevice: "Este dispositivo",
    otherDevice: "“Mover” no sale de un dispositivo: la pantalla “Transferencia” copia entre dispositivos.",
    loadingDevices: "Cargando dispositivos",
    noDevice: "Ningún dispositivo conectado",
    noDeviceHint: "Vincula una PS5 o un ordenador, o añade un NAS, para gestionar sus archivos aquí.",
    openConnect: "Abrir “Conectar”",
    dropMove: (folder) => `Suelta para mover a ${folder}`,
    cantCopy: "No se puede copiar:",
    copyNoFolder: "Abre primero una carpeta en ese dispositivo.",
    copyOne: (name: string, to: string) => `Copiando ${name} en ${to}. La transferencia aparece en “Cola”.`,
    copyMany: (n: number, to: string) =>
      plural(n, {
        one: `Copiando ${num(n)} elemento en ${to}. La transferencia aparece en “Cola”.`,
        many: `Copiando ${num(n)} de elementos en ${to}. La transferencia aparece en “Cola”.`,
        other: `Copiando ${num(n)} elementos en ${to}. La transferencia aparece en “Cola”.`,
      }),
  },
  // build/ (Build's pack settings and checks), JobList.tsx's pack tags
  pack: {
    help: "Cómo funciona empaquetar",
    patch: "Un paquete de parche no se puede empaquetar.",
    additionalContent: "El contenido adicional (sin eboot.bin) no se puede empaquetar.",
    levels: { fast: "Rápido", medium: "Medio", max: "Máximo" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Recomendado. ZSTD 5: rápido, buen tamaño." },
        medium: { text: "ZSTD 9: más pequeño, más lento." },
        max: { text: "ZSTD 19: el más pequeño; mucho más lento, más memoria." },
      },
      lz4: {
        fast: { tone: "good", text: "Recomendado. LZ4 por defecto: el más rápido." },
        medium: { text: "LZ4 HC 9: más pequeño, más lento." },
        max: { text: "LZ4 HC 12: el más pequeño, el más lento." },
      },
    },
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index está desactualizado" : state === "damaged" ? "ampr_emu.index está dañado" : "No hay ampr_emu.index"}: al empaquetar se escribe uno nuevo.`,
    rules: {
      title: "Reglas propias (TOML)",
      titleWith: (name) => `Reglas propias (TOML): ${name}`,
      info: "Un perfil de la estrategia de empaquetado en el formato de ampr_pack.py de ampr_emu y Lazy_AMPR decide qué archivos se empaquetan y cuáles quedan sueltos. Gana la última regla que coincide; los archivos del sistema siempre quedan sueltos.",
      warn: "Unas reglas erróneas pueden dejar un juego que no arranca. Guarda una copia.",
      pickTitle: "Estrategia de empaquetado (archivo TOML)",
      clear: "Quitar",
    },
  },
  // PackHelp.tsx
  packHelp: {
    title: "Cómo funciona empaquetar",
    close: "Cerrar",
    body: () => (
      <>
        <h3>ZSTD</h3>
        <ol>
          <li>Empaqueta los recursos del juego en volúmenes ZSTD, en una carpeta o una imagen.</li>
          <li><b>Rápido</b> (ZSTD 5) es veloz; <b>Medio</b> (9) y <b>Máximo</b> (19) son más pequeños y más lentos.</li>
          <li>Para un juego empaquetado con ZSTD, <b>Ninguno</b> lo desempaqueta: vuelven los archivos normales.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Puede que algunos juegos no arranquen.</b> Guarda una copia.</li>
          <li>Empaqueta en volúmenes LZ4, en una carpeta o una imagen: <b>Rápido</b> (LZ4), <b>Medio</b> (HC 9), <b>Máximo</b> (HC 12).</li>
          <li>Funciona con ampr-emu-ng, igual que los paquetes ZSTD.</li>
          <li>Para un juego empaquetado con LZ4, <b>Ninguno</b> lo desempaqueta.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>El único runtime de AMPR que incluye Forge. Lee paquetes ZSTD y LZ4; al empaquetar se instala en fakelib/.</li>
          <li>Al crear un juego, Forge sustituye su ampr_emu por ampr-emu-ng, salvo que Ajustes conserve ampr_emu. Un FPKG no lleva runtime: lo proporciona el cargador de la consola.</li>
          <li>Desactiva la actualización de fakelib de ShadowMount+, o reemplazará el runtime y el juego no podrá leer sus paquetes.</li>
        </ol>

        <h3>Estrategia de empaquetado</h3>
        <ol>
          <li><b>Empaquetar todo salvo los archivos del sistema</b> (ZSTD y LZ4): todos los archivos salvo eboot.bin, ejecutables y módulos, sce_sys, sce_module, fakelib y los archivos .ini (dlc_emu.ini sigue siendo editable); ampr-emu-ng sirve el resto, audio y vídeo incluidos.</li>
          <li>Al empaquetar, en <b>Reglas propias (TOML)</b>, elige un perfil en el formato de ampr_pack.py y Lazy_AMPR, en este dispositivo o en otro.</li>
          <li>Gana la última regla que coincide; los archivos del sistema siempre quedan sueltos. Unas reglas erróneas pueden dejar un juego que no arranca.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "Dispositivos",
    thisDevice: "Este dispositivo",
    drives: "Unidades",
    location: "Ubicación",
    parent: "Carpeta superior",
    allDrives: "Todas las unidades",
    truncated: (n) => plural(n, {
  one: "Solo se muestra la primera entrada.",
  many: `Solo se muestran las primeras ${num(n)} entradas.`,
  other: `Solo se muestran las primeras ${num(n)} entradas.`,
}),
    loading: "Cargando",
    noDrives: "No se encontraron unidades.",
    noFolders: "No hay carpetas aquí.",
    noImages: "No hay carpetas, imágenes de PS5 ni archivos comprimidos aquí.",
    noFiles: (ext) => `No hay carpetas ni archivos ${dotted(ext, " o ")} aquí.`,
    chooseFiles: (n) => (n === 0 ? "Elegir archivos" : `Elegir ${num(n)} ${plural(n, FILES)}`),
    nothing: "No hay carpetas ni archivos aquí.",
    cantList: "No se puede abrir esta carpeta:",
    cancel: "Cancelar",
    chooseFolder: "Elegir esta carpeta",
    netOnlyHere: "Archivos comprimidos y FPKG: solo desde este dispositivo.",
  },
  // bookmarks.tsx: the bookmarked folders (Transfer and the File explorer)
  bookmarks: {
    label: "Marcadores",
    add: "Añadir esta carpeta a marcadores",
    remove: "Quitar marcador",
    removeNamed: (name: string) => `Quitar marcador ${name}`,
    full: "9 marcadores como máximo",
  },
  // DeviceTabs.tsx
  deviceTabs: {
    add: "Añadir un dispositivo",
    offline: "Sin conexión",
    offlineNamed: (name: string) => `${name} · Sin conexión`,
  },
  // common.tsx
  common: {
    folder: "Carpeta",
    pickButton: "Elegir",
    pickAny: "Carpeta, imagen o archivo comprimido del juego",
    source: "Origen",
    emptyTitle: "Elige una carpeta, imagen o archivo comprimido de juego",
    accepted: "Orígenes admitidos",
    unknownTitle: "Título desconocido",
    unsigned: (n: number) =>
      plural(n, {
        one: `${num(n)} ejecutable no está firmado, así que el juego no arrancará. Al convertir el juego, se firma.`,
        many: `${num(n)} de ejecutables no están firmados, así que el juego no arrancará. Al convertir el juego, se firman.`,
        other: `${num(n)} ejecutables no están firmados, así que el juego no arrancará. Al convertir el juego, se firman.`,
      }),
    patch: "Parche",
    additionalContent: "Contenido adicional",
    reading: "Leyendo el origen",
    unreadable: "Origen ilegible",
    backport: "Backport",
    emulators: "Emuladores",
    declares: (fw) => `declara ${fw}`,
    fwUnknown: "firmware desconocido según sus ejecutables (no hay SDK legible)",
    emulatorList: (names) => `emuladores: ${names}`,
    otherEmulator: "Otro",
    sdk: (v) => `SDK ${v} (declarado)`,
    titleId: "ID del título",
    version: "Versión",
    firmware: "Firmware",
    fwDeclared: "declarado",
    fwBackport: "backport",
    fwExecutables: "ejecutables",
    size: "Tamaño",
    files: (n) => `${num(n)} ${plural(n, FILES)}`,
    contentId: "ID de contenido",
    noAmpr: "eboot.bin no importa libSceAmpr",
    traceRuntime: "Runtime de trazado LZ4 antiguo",
    noRuntime: "Sin runtime",
    packed: (kind) => `Empaquetado (${kind})`,
    zstdKept: "paquetes de recursos ZSTD: la conversión los conserva",
    damaged: "Paquetes dañados",
    unpacked: "Sin empaquetar",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} en ${num(volumes)} ${plural(volumes, { one: "volumen", many: "volúmenes", other: "volúmenes" })}` +
      (percent === null ? "" : `, ${num(percent)}% del tamaño`),
    index: {
      stale: "ampr_emu.index desactualizado: una conversión lo renueva",
      damaged: "ampr_emu.index dañado: una conversión lo reescribe",
      missing: "sin ampr_emu.index: una conversión lo escribe",
    },
    runtimeAction: (a: "install" | "replace" | "keep" | "keepIfPacks") =>
      a === "install"
        ? "una conversión instala el ampr-emu-ng incluido"
        : a === "replace"
          ? "una conversión sustituye el runtime por el ampr-emu-ng incluido"
          : a === "keep"
            ? "una conversión lo conserva (Ajustes)"
            : "una conversión lo conserva si lee paquetes LZ4; si no, lo sustituye por ampr-emu-ng",
    backportLibs: (exclusive, n) =>
      `${exclusive ? "fakelib2/, montado por separado" : "fakelib/"}, ${num(n)} ${plural(n, { one: "biblioteca", many: "bibliotecas", other: "bibliotecas" })} de backport`,
    findings: "Avisos de la comprobación previa",
    dlcTitle: (n) => `DLC incluidos (${num(n)})`,
    dlcLabel: "DLC incluidos",
    dlcAll: (note) => `Cada DLC: ${note}.`,
    dlcIn: (folder, size) => `en ${folder}/, ${size}`,
    dlcNoData: "desbloqueado por el emulador de DLC (dlc_emu.ini): solo la licencia, sin datos propios",
    dlcStatus: (status) => `desbloqueado por el emulador de DLC (dlc_emu.ini), estado ${status || "sin definir"}`,
    dlcMerged: "integrado en las carpetas del juego",
    backportFiles: "Archivos del backport",
    emulatorsIn: "Emuladores en fakelib",
    qr: (url) => `Código QR: ${url}`,
    needsPassword: "Este archivo comprimido necesita una contraseña.",
    enterPassword: "Introducir contraseña",
    inArchive: (ext, size) => `en un archivo ${ext} · ${size}`,
    archive: "Archivo comprimido",
    parts: (n) => `${num(n)} ${plural(n, { one: "parte", many: "partes", other: "partes" })}`,
    archiveRoot: (root) => `juego en ${root}`,
    archiveAtRoot: "juego en la raíz del archivo",
    solid: "sólido",
    encrypted: "protegido con contraseña",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "¿Eliminar esta carpeta?" : "¿Eliminar esta imagen?"),
    text: (folder) =>
      folder
        ? "La carpeta y todo su contenido se eliminarán para siempre: sin papelera ni forma de deshacerlo. Si la eliminación se detiene a medias, una parte puede haber desaparecido ya."
        : "El archivo de imagen se eliminará para siempre: sin papelera ni forma de deshacerlo.",
    no: "No",
    yes: "Sí, eliminar",
    archiveTitle: "¿Eliminar este archivo comprimido?",
    archiveText:
      "El archivo comprimido se eliminará para siempre: sin papelera ni forma de deshacerlo. Las demás partes de un archivo en varias partes se quedan como están.",
    fileTitle: "¿Eliminar este archivo?",
    fileText: "El archivo se eliminará para siempre: sin papelera ni forma de deshacerlo.",
  },
  // replace.tsx
  replace: {
    title: (n) =>
      plural(n, { one: `${num(n)} elemento ya existe allí`, other: `${num(n)} elementos ya existen allí` }),
    inFolder: (folder) => <>En {folder}:</>,
    more: (n) => `y ${num(n)} más`,
    text: "“Reemplazar” pone los nuevos en su lugar: los archivos anteriores desaparecen para siempre, sin Papelera y sin deshacer. Una carpeta sobre otra carpeta con el mismo nombre se combina con ella: los archivos con el mismo nombre se reemplazan y todo lo demás que hay allí se queda.",
    cancel: "Cancelar",
    replace: "Reemplazar",
  },
  // password.tsx
  password: {
    title: "Este archivo comprimido necesita una contraseña",
    text: "Introduce su contraseña para abrirlo.",
    wrong: "La contraseña no es correcta. Revísala e inténtalo de nuevo.",
    label: "Contraseña",
    cancel: "Cancelar",
    unlock: "Desbloquear",
    checking: "Comprobando",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "No se puede contactar con el cargador de payloads en el puerto 9021. Ejecuta primero tu autoloader (jailbreak) y luego vuelve a abrir Dump Forge.",
      missing:
        "No se encontró ninguna copia guardada de Dump Forge (ni en /data/dumpforge/ ni en una unidad USB). Cárgalo una vez desde un PC; guardará una copia para este icono.",
      timeout:
        "Dump Forge se inició pero no ha respondido tras 30 segundos. Puede que aún se esté iniciando (una consola que sale del modo de reposo tarda más).",
      stopping: "El Dump Forge anterior todavía se está deteniendo (cancela sus trabajos y limpia). Espera un momento y vuelve a intentarlo.",
    },
    loadFailed: (e) => `No se pudo cargar la página: ${e}`,
    noAnswer: "Dump Forge no respondió",
    cantStart: "No se puede iniciar Dump Forge",
    retry: "Reintentar",
    starting: "Iniciando Dump Forge…",
    client: {
      unreachable:
        "No se puede contactar con Dump Forge. Comprueba que el payload sigue en ejecución y que este dispositivo está en la misma red.",
      noPlatform: "No se puede saber si Dump Forge se ejecuta en una PS5. Actualiza la página para volver a intentarlo.",
      quitUnreachable: "No se puede contactar con Dump Forge para detenerlo.",
      stillAnswering: "Dump Forge sigue respondiendo tras 2 minutos; revisa la PS5.",
      gone: "Dump Forge ya no informa de ella (salió de su historial de tareas antes de que se viera su resultado)",
      uploadIncomplete: "La subida no se completó.",
    },
  },
  // target.tsx
  target: {
    kraken: {
      fast: { label: "Rápido", note: { tone: "good", text: "Recomendado. La creación más rápida." } },
      balanced: {
        label: "Equilibrado",
        note: {
          text: "Un 2,6% más pequeño que “Rápido”, pero comprimir tarda unas 6 veces más, con todos los núcleos ocupados todo el tiempo.",
        },
      },
      smallest: {
        label: "Mínimo",
        note: {
          text: "Un 2,7% más pequeño que “Rápido”, pero comprimir tarda unas 9 veces más, con todos los núcleos ocupados todo el tiempo.",
        },
      },
      stored: {
        label: "Sin compresión",
        note: {
          text: "El paquete ocupa más o menos lo mismo que el juego. Pruébalo si un paquete comprimido se instala pero no arranca (se ha visto con firmware 8.60 y 10.40).",
        },
      },
    },
    imageInside: "Imagen interior",
    imageInsideLabel: "Imagen dentro del ffpfsc",
    compressionLevel: "Nivel de compresión",
    compression: "Compresión",
    fullVerify: "Verificación completa",
    fullVerifyHint: "Vuelve a leer cada byte para comprobar la salida. Tarda más.",
    deleteSource: "Eliminar el origen",
    deleteSourceHint: "No se puede deshacer. El origen se elimina solo después de una conversión correcta.",
    deleteSourceNas: "Un juego en un NAS no se puede eliminar desde aquí.",
    separator: "Un nombre no puede contener un separador de carpetas; usa “Cambiar” para la carpeta.",
    badName: "Un nombre no puede estar vacío, empezar por un punto, terminar en punto o espacio, ni contener caracteres de control.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Demasiado largo: ${num(bytes)} B, como máximo ${num(max)} B (UTF-8).`
        : `Demasiado largo: ${num(bytes)} B antes de la extensión, como máximo ${num(max)} B (UTF-8).`,
    pkgExt: (ext) => `Termina en .${ext}, pero FPKG se guarda como .pkg: la extensión determina el controlador.`,
    otherExt: (ext, target) =>
      `Termina en .${ext}, pero el destino es ${target}: la extensión determina el controlador.`,
    extractInto: "Extraer en esta carpeta",
    saveInto: "Guardar en esta carpeta",
    output: "Salida",
    change: "Cambiar",
    naming: "Generando nombre…",
    chooseSourceFirst: "Elige primero un origen",
    fileName: "Nombre de archivo",
    savedAs: (name, dir) => `se guarda como ${name} en ${dir}`,
    inDir: (dir) => `en ${dir}`,
    generate: "Generar nombre según el contenido",
  },
  // Formats.tsx (About's Output names) and target.tsx's name pattern: generated output names
  naming: {
    title: "Nombres de salida",
    parts: { title: "Título", title_id: "ID del título", version: "Versión", pack: "Tipo de paquete", region: "Región", fw: "Firmware" },
    template: "Plantilla del nombre",
    partsLabel: "Insertar una parte",
    insert: (part: string) => `Insertar ${part}`,
    inUse: (part) => `${part} ya está en la plantilla`,
    example: "Ejemplo",
    length: "Longitud",
    bytes: (n, max) => `${n} de ${max} bytes.`,
    room: (n) => plural(n, { one: `Queda ${num(n)} byte para el título.`, other: `Quedan ${num(n)} bytes para el título.` }),
    noRoom: "No queda espacio para el título: se omite.",
    versionOut: "La versión se omite para hacer sitio.",
    cut: "El nombre se corta a 63 bytes.",
    idOnly: "El texto de la plantilla ocupa los 63 bytes, así que el ID del título nombra la salida.",
    errors: {
      long: (max: number) => `Una plantilla tiene como máximo ${max} bytes.`,
      open: "Una { no tiene su } de cierre.",
      close: "Una } no tiene su { de apertura.",
      unknown: (token: string) => `{${token}} no es una parte: usa Título, ID del título, Versión, Tipo de paquete, Región o Firmware.`,
      twice: (token) => `{${token}} aparece dos veces en la plantilla: usa cada parte una sola vez.`,
      char: (c: string) => `Un nombre no puede contener ${c}.`,
      control: "Un nombre no puede contener un carácter de control.",
      needsName: "Un nombre necesita Título o ID del título.",
    },
    notSaved: "No se guardó: los nombres siguen usando la última plantilla válida.",
    reset: "Restablecer",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} está disponible`,
    update: "Actualizar",
    later: "Más tarde",
    updating: "Actualizando…",
    updatingNote: "La nueva versión se está descargando. Esta página se recargará sola con ella.",
    downloading: "La descarga se abre en tu navegador: reemplaza esta copia con el archivo descargado (en un Mac, arrastra la app del .dmg a Aplicaciones).",
    downloadingCli: "La descarga se abre en el navegador: sustituye la CLI de ps5-dump-forge por el archivo descargado o ejecuta ps5-dump-forge update --install en el ordenador donde se ejecuta.",
    failed: (v) => `No se pudo actualizar a la v${v}`,
    timeout: (v) => `La v${v} no se inició en un minuto. Si se descargó, detén Dump Forge y vuelve a abrirlo para terminar la actualización.`,
    retry: "Reintentar",
    close: "Cerrar",
  },
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "Este dispositivo",
    howItWorks: "Cómo conectar",
    ipAddress: "Dirección IP",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? plural(n, { one: `visto: hace ${num(n)} minuto`, many: `visto: hace ${num(n)} minutos`, other: `visto: hace ${num(n)} minutos` })
        : unit === "h"
          ? plural(n, { one: `visto: hace ${num(n)} hora`, many: `visto: hace ${num(n)} horas`, other: `visto: hace ${num(n)} horas` })
          : unit === "d"
            ? plural(n, { one: `visto: hace ${num(n)} día`, many: `visto: hace ${num(n)} días`, other: `visto: hace ${num(n)} días` })
            : "visto: ahora mismo",
    listenError: (e) => `Los otros dispositivos no pueden conectar con este: ${e}`,
    unavailable: (e) => `Conectar no está disponible: ${e}`,
    loading: "Cargando",
    discover: "Detección automática",
    stop: "Detener",
    fileTransfer: "Transferencia de archivos",
    devices: "Dispositivos",
    show: "Mostrar",
    all: "Todos",
    onlineOf: (n, total) => `${num(n)} de ${num(total)} en línea`,
    sendFilesTo: (name) => `Enviar archivos a ${name}`,
    userIs: (user) => `usuario ${user}`,
    nearby: "Cerca",
    nearbyNone: "No se encontraron dispositivos nuevos",
    addDevice: "Añadir un dispositivo",
    addressExample: "192.168.1.20 o nas.local",
    addByProtocol: "O añadir por protocolo:",
    savedEmpty: "Aún no hay dispositivos guardados",
    savedEmptyHint:
      "La detección automática encuentra Forge y almacenamiento en red en esta red. Vincula un dispositivo y permítelo allí, o guarda un NAS.",
    online: "En línea",
    offline: "Sin conexión",
    computer: "Ordenador",
    address: "Dirección",
    keyId: "ID de clave",
    lastSeen: "Visto por última vez",
    status: "Estado",
    never: "Nunca",
    unknown: "Desconocido",
    forget: "Olvidar",
    forgetNote: "Olvidar elimina la vinculación solo en este dispositivo. Para volver a usarlo, vincúlalo de nuevo.",
    close: "Cerrar",
    cancel: "Cancelar",
    pairTitle: "Vincular un dispositivo",
    pair: "Vincular",
    connectTo: "Conectar",
    probing: (address) => `Comprobando ${address}`,
    pairing: "Vinculando",
    waitingFor: (address) => `Esperando a que ${address} permita la vinculación`,
    checkNumber: "Número de comprobación",
    checkHint: "Debe coincidir con el número que muestra el otro dispositivo.",
    denied: "El otro dispositivo rechazó la vinculación.",
    timedOut: "Nadie permitió la vinculación en el otro dispositivo a tiempo.",
    pairBusy: "El otro dispositivo tiene otras solicitudes de vinculación en espera. Vuelve a intentarlo en un minuto.",
    alreadyPaired: "Ese dispositivo ya tiene este guardado. Olvídalo allí primero y vuelve a vincular.",
    tooManyAttempts: "Demasiadas solicitudes rechazadas. Vuelve a intentarlo en 10 minutos.",
    pairPromptTitle: (name) => `¿Vincular con ${name}?`,
    pairPromptBody: (name, platform, ip) =>
      `${name} (${platform}, ${ip}) quiere vincularse con este dispositivo. Un dispositivo vinculado puede leer los archivos de este dispositivo y añadir otros nuevos.`,
    pairPromptCheck: (name) => `Permítelo solo si ${name} muestra el mismo número de comprobación.`,
    allow: "Permitir",
    deny: "Rechazar",
    notPaired: "El otro dispositivo ya no reconoce a este. Vincúlalo de nuevo.",
    searching: "Buscando en esta red",
    forge: "Forge",
    save: "Guardar",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `Añadir un NAS ${protocol}`,
    nasEditTitle: (protocol) => `Editar NAS ${protocol}`,
    nasName: "Nombre",
    nasAddressHint: "Dirección IP o nombre de host; añade :puerto si no es 445",
    nasUser: "Nombre de usuario",
    nasUserHint: "Déjalo vacío para entrar como invitado",
    ftpAddressHint: "Dirección IP o nombre de host; añade :puerto si no es 21",
    nfsAddressHint: "Dirección IP o nombre de host; añade :puerto si no es 2049",
    ftpUserHint: "Déjalo vacío para entrar de forma anónima",
    ftpNote: "FTP no está cifrado, tampoco el inicio de sesión.",
    nfsNote: "NFS no tiene inicio de sesión: Forge se conecta como root (NFSv4.1, si no NFSv3), así que la exportación del servidor debe permitir este dispositivo. NFS no está cifrado.",
    nasPassword: "Contraseña",
    nasPasswordKeep: "Déjalo vacío para conservar la contraseña guardada",
    guest: "Invitado",
    passwordSaved: "Guardada",
    passwordNone: "Ninguna",
    edit: "Editar",
    nasForgetNote: "Olvidar elimina este NAS y su contraseña guardada de este dispositivo.",
    thisBrowser: "Este navegador",
    uploadPick: "Elegir archivos para subir",
    uploading: (name, i, n) => `Subiendo ${name} (${num(i)} de ${num(n)})`,
    uploaded: "Subida terminada",
    uploadSkipped: (names) => `No se subió, allí hay una carpeta o un enlace con ese nombre: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `Se subió ${num(done)} de ${num(n)} archivos y se queda en este dispositivo.`,
        other: `Se subieron ${num(done)} de ${num(n)} archivos y se quedan en este dispositivo.`,
      }),
    cantUpload: "No se puede subir:",
    send: "Enviar",
    downloadHere: "Descargar de este dispositivo",
    wholeRoot: "No se puede enviar un dispositivo, recurso compartido o unidad entero: elige una carpeta dentro.",
    helpPhone: "Escanea el código QR para abrir esta página en un teléfono. Allí, Transferencia sube los archivos del teléfono a este dispositivo y descarga al teléfono los archivos de este dispositivo.",
    help: [
      "Conectar une Dump Forge de tu PS5 y tus ordenadores en la misma red.",
      "Para vincular, inicia la Detección automática y elige Vincular en Cerca, o escribe una dirección en Añadir un dispositivo, y permítelo en el otro dispositivo. Permítelo solo si ambas pantallas muestran el mismo número de comprobación.",
      "Un NAS (SMB, FTP, NFS) se añade igual. Los dispositivos vinculados y los NAS sirven como origen de juegos y como destino; sus juegos no se editan allí.",
      "Transferencia, en el menú lateral, copia archivos y carpetas entre cualquiera de ellos. Un nombre que ya existe allí solo se reemplaza después de que lo confirmes.",
    ],
    helpSecurity: "Nada de lo que se envía entre dispositivos está cifrado: cualquiera en tu red puede leerlo. La contraseña guardada de un NAS se queda en este dispositivo sin cifrar. Usa Conectar solo en tu red doméstica.",
    helpSpeed: "Prueba de velocidad, en la fila de un dispositivo vinculado o en sus detalles, le envía 128 MiB y recibe 128 MiB de vuelta, sin escribir nada en disco, y muestra la velocidad de la red en cada sentido.",
    speedTest: "Prueba de velocidad",
    speedTestWith: (name) => `Prueba de velocidad con ${name}`,
    speedTesting: "Probando",
    speedResult: (up, down) => `Subida ${up} MB/s · Bajada ${down} MB/s`,
    speedFailed: "La prueba de velocidad falló:",
    speedNote: "Envía 128 MiB y recibe 128 MiB de vuelta; no se escribe nada en disco.",
    installTitle: "Dump Forge en la PS5",
    ps5Address: "Dirección de la PS5",
    install: "Instalar en la PS5",
    installHint: "Necesita el cargador ELF de la consola accesible en la red (puerto 9021).",
    findingRelease: "Buscando la versión en GitHub",
    installing: (v, host) => `Instalando Dump Forge ${v} en ${host}`,
    installed: (v, host) => `Dump Forge ${v} se está iniciando en ${host}. Vincúlalo desde Añadir un dispositivo cuando esté en marcha.`,
    noRelease: "No se encuentra la versión de Dump Forge para PS5 en GitHub. Comprueba la conexión a internet de este ordenador y vuelve a intentarlo.",
    otherPayload: "Cualquier otro payload ELF:",
    sendPayload: "Enviar un payload",
    pickPayload: "Elige un payload ELF",
    sendingPayload: (name, host) => `Enviando ${name} a ${host}`,
    sentPayload: (name, size, host) => `${name} (${size}) enviado a ${host}.`,
    helpInstallTitle: "Instala Dump Forge en una PS5 con un clic",
    helpInstallText: "Usa la tarjeta para instalar Dump Forge y enviar el payload a la consola. Descarga de GitHub la versión correspondiente (o la más reciente) y la inicia.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "Instalador de paquetes",
    packages: "Paquetes",
    refresh: "Actualizar",
    scanning: "Buscando paquetes",
    none: "No se encontraron paquetes en esta consola.",
    noneHint: "Copia un archivo .pkg al almacenamiento de la consola o a una unidad USB y pulsa Actualizar.",
    truncated: "Puede que la lista esté incompleta",
    limits: "La búsqueda se detuvo en uno de sus límites: puede que falten algunos paquetes.",
    cantScan: "No se puede buscar:",
    cantInstall: "No se puede instalar:",
    content: { game: "Juego", patch: "Parche", additional_content: "Contenido adicional", unknown: "Desconocido" },
    install: "Instalar",
    installName: (name) => `Instalar ${name}`,
    started: (name: string) => `Instalando ${name}. La tarea aparece en “Cola”.`,
    tag: "Instalación",
    title: "¿Instalar este paquete?",
    titleOn: (device) => `¿Instalar este paquete en ${device}?`,
    file: "Archivo",
    game: "Juego",
    titleId: "ID del título",
    version: "Versión",
    size: "Tamaño",
    text: "La PS5 obtiene el paquete de Dump Forge y lo instala. Mantén Dump Forge abierto hasta que termine la tarea.",
    reinstall: "Si este juego ya está instalado, la PS5 borra la copia instalada antes de instalar esta.",
    cancel: "Cancelar",
    confirm: "Instalar",
    installPkg: "Instalar pkg",
    installPkgOn: (device) => `Instalar un paquete en ${device}`,
    pickTitle: "Elige un paquete para instalar",
    sent: (device: string) => `${device} está instalando el paquete. La tarea aparece en “Cola”.`,
  },
  // Build's and Edit's Restore metadata online switch (build/options.tsx, Edit.tsx)
  metadata: {
    tag: "Metadatos",
    cant: "No se pudieron restaurar los metadatos:",
    label: "Restaurar metadatos en línea",
    buildHint: "Antes de compilar, descarga del servidor de actualizaciones de Sony el param.json, las imágenes, el sonido y las tablas PlayGo de esta versión y los pone en el resultado donde difieren. El juego no se cambia. Necesita internet; sin él la compilación sigue tal cual.",
    buildSummary: "Pone los metadatos de Sony en el resultado (el juego no se cambia)",
    editHint: "Aplica a este juego, con tus cambios, el param.json, las imágenes, el sonido y las tablas PlayGo de esta versión desde el servidor de actualizaciones de Sony donde difieren: una carpeta se cambia directamente, un exfat o ffpkg en su sitio, y ffpfs, ffpfsc o FPKG se reconstruyen. Necesita internet; si la descarga falla, no se cambia nada.",
    editOnly: "Los metadatos de Sony de esta versión, aplicados a este juego",
    editAlso: "También se aplican los metadatos de Sony de esta versión, descargados antes.",
    editConfirm: "¿Aplicar los metadatos de Sony?",
    editConfirmText: "Primero se descargan los metadatos; si eso falla, no se cambia nada.",
  },
  // App.tsx: the side rail
  rail: {
    build: "Crear",
    settings: "Ajustes",
    queue: "Cola",
    transfer: "Transferencia",
    collapse: "Contraer menú",
    expand: "Expandir menú",
  },
  // Queue.tsx
  queue: {
    empty: "Aún no hay tareas. Aquí aparecen las creaciones, ediciones, transferencias e instalaciones.",
    counts: (running: number, queued: number, failed: number, done: number): string =>
      [
        running && `${num(running)} en curso`,
        queued && `${num(queued)} en cola`,
        failed && plural(failed, { one: `${num(failed)} fallida`, many: `${num(failed)} fallidas`, other: `${num(failed)} fallidas` }),
        done && plural(done, { one: `${num(done)} terminada`, many: `${num(done)} terminadas`, other: `${num(done)} terminadas` }),
      ]
        .filter(Boolean)
        .join(", ") || "Nada que mostrar",
    retryFailed: (n: number): string => (n > 0 ? `Reintentar fallidas (${num(n)})` : "Reintentar fallidas"),
    retrying: (i: number, n: number) => `Reintentando ${num(i)} de ${num(n)}`,
    clearFinished: "Quitar terminadas",
    filters: { all: "Todas", running: "En curso", failed: "Fallidas", done: "Terminadas" },
    filterLabel: "Mostrar",
    noneShown: "Ninguna tarea coincide con este filtro.",
  },
  // Settings.tsx
  settings: {
    appearance: "Apariencia",
    theme: "Tema",
    themeHint: "El oscuro va bien en un televisor; el claro, en una habitación luminosa.",
    dark: "Oscuro",
    light: "Claro",
    scheme: "Esquema de colores",
    schemeHint: "El color de acento de toda la aplicación.",
    schemes: { abyss: "Abismo", ember: "Brasa", moss: "Musgo", cobalt: "Cobalto", dusk: "Crepúsculo", rose: "Rosa", sand: "Arena", volt: "Volt", mono: "Mono" },
    advanced: "Opciones avanzadas",
    advancedOpen: "Mostrar siempre las opciones avanzadas",
    advancedHint:
      "Muestra todos los paquetes y formatos, sin “Más”, y mantiene abiertas todas las secciones. Desactívalo para que Crear sea breve.",
    bundled: (version: string) => `${version} (incluido)`,
    replaceAmprEmu: "Reemplazar ampr_emu por ampr-emu-ng",
    installedGames: "Mostrar juegos instalados",
    installedGamesHint:
      "Muestra los juegos instalados (/user/app) junto a /data en los selectores y el explorador de archivos, para que el app.pkg de un FPKG instalado sirva de origen para crear o editar. Solo lectura: allí no se cambia ni se borra nada. Un juego comercial no se puede leer.",
    languageHint: "Los idiomas del sistema hasta que elijas uno.",
    notificationsGroup: "PS5",
  },
  // JobList.tsx: a finished job's result
  result: {
    file: "Salida",
    type: "Tipo",
    size: "Tamaño",
    files: "Archivos",
    time: "Tiempo",
    verified: "Verificado",
    verifiedFast: "Verificado (rápido)",
    verifiedFull: "Verificado (completo)",
    notVerified: "No se informó ninguna comprobación",
    checks: "Comprobaciones:",
    copy: "Copiar detalles",
    copied: "Copiado",
    copyFailed: "No se puede copiar aquí",
    ofSource: (pct: number) => `${num(pct, 1)}% de los archivos del juego`,
    filesN: (n: number) => plural(n, { one: `${num(n)} archivo`, many: `${num(n)} archivos`, other: `${num(n)} archivos` }),
    to: "escrito en",
    took: (d: string) => `tardó ${d}`,
    kinds: {
      pkg: "FPKG: se instala en una PS5 compatible con FPKG",
      exfat: "Imagen exfat: ShadowMount+ la monta, con escritura",
      ffpkg: "Imagen ffpkg (UFS2): ShadowMount+ la monta, con escritura",
      ffpfs: "Imagen ffpfs (PFS): ShadowMount+ la monta",
      ffpfsc: "Imagen ffpfsc (PFS comprimido): ShadowMount+ la monta solo lectura",
      folder: "Carpeta del juego",
    } as Record<string, string | undefined>,
    zstdFolder: "Carpeta empaquetada con ZSTD",
    lz4Folder: "Carpeta empaquetada con LZ4",
    withPacks: (image: string, kind: string) => `${image}, archivos en paquetes ${kind}`,
    edited: (what: string) => `${what}, cambiado en el sitio`,
    rebuilt: (what: string) => `${what}, reconstruido`,
    extracted: "Archivos extraídos",
    transferred: "Archivos copiados",
    installed: "Paquete entregado al instalador de la PS5",
  },
  // JobList.tsx: the log panel
  log: {
    lines: (n: number, of?: number): string =>
      of === undefined
        ? plural(n, { one: `${num(n)} línea`, many: `${num(n)} líneas`, other: `${num(n)} líneas` })
        : plural(of, { one: `${num(n)} de ${num(of)} línea`, many: `${num(n)} de ${num(of)} líneas`, other: `${num(n)} de ${num(of)} líneas` }),
    filter: "Filtrar",
    follow: "Desplazamiento automático",
    copy: "Copiar",
    copied: "Copiado",
    copyFailed: "No se pudo copiar",
    save: "Guardar",
    clear: "Borrar",
    cleared: "Borrado",
  },
  // common.tsx: the source card's fact chips
  sourceChips: {
    sdk: (v: string) => `SDK ${v}`,
    firmware: (v: string) => `FW ${v}+`,
  },
  // prefs.tsx: Advanced's badge
  advanced: {
    changed: "Cambiadas",
    defaults: "Por defecto",
  },
  // build/*: the Build screen
  build: {
    summary: "Resumen",
    addToQueue: "Añadir a la cola",
    queued: "Añadido a la cola",
    openQueue: "Abrir la cola",
    space: (free: string, dir: string, needed: string) => (
      <>
        {free} libres en <span className="path">{dir}</span>, hacen falta unos {needed}
      </>
    ),
    needed: (needed: string) => `Hacen falta unos ${needed}`,
    spaceRaw: "El espacio se comprueba como si nada se comprimiera.",
    estimate: "estimación",
    advanced: "Opciones avanzadas",
    options: "Opciones",
    fastVerify: "Verificación rápida: se comprueba cada estructura y se relee una muestra de los datos.",
    fullVerify: "Verificación completa: se relee cada byte.",
    deleted: "El origen se elimina tras una creación correcta. No se puede deshacer.",
    zstdBackport:
      "Los recursos de este juego están empaquetados con ZSTD y su índice ZSTD incluye el backport: créalo una vez con Paquetes: Ninguno y luego quita el backport.",
    lz4Backport:
      "Los recursos de este juego están empaquetados con LZ4 y su manifiesto LZ4 incluye el backport: créalo una vez con Paquetes: Ninguno y luego quita el backport.",
    backportLeftOut: (n: number) =>
      plural(n, {
        one: `Se omite ${num(n)} biblioteca de backport de fakelib/.`,
        many: `Se omiten ${num(n)} bibliotecas de backport de fakelib/.`,
        other: `Se omiten ${num(n)} bibliotecas de backport de fakelib/.`,
      }),
    packsTitle: "Destino",
    packs: "Paquetes",
    more: "Más",
    moreTip: "Más formatos",
    cards: {
      folder: "Archivos normales, cópialos donde quieras, ShadowMount+ lo ejecuta",
      pkg: "Se instala en la PS5 como un paquete de juego",
      exfat: "Se monta con ShadowMount+ y se abre en un PC",
      ffpkg: "Imagen UFS2 para ShadowMount+, admite montajes con escritura",
      ffpfs: "Imagen PFS, sin comprimir, solo nombres de archivo ASCII",
      ffpfsc: "La imagen más pequeña: comprimida, siempre montada solo lectura",
      lz4: "Recursos empaquetados con LZ4, carpeta normal",
    },
    zstdFolderCard: "Recursos empaquetados con ZSTD, carpeta normal",
    packCards: {
      nonePlain: "Los archivos del juego tal como están",
      unpack: "Vuelve a escribir los paquetes como archivos normales",
      keepZstd: "Los paquetes ZSTD se conservan byte a byte",
      keepLz4: "Los paquetes LZ4 se conservan byte a byte",
      zstd: "Juego más pequeño, carga más rápida, con ampr-emu-ng",
      repackZstd: "Decodifica los paquetes LZ4 y vuelve a empaquetar con ZSTD",
      lz4: "Empaqueta los recursos con LZ4, con ampr-emu-ng",
      repackLz4: "Decodifica los paquetes ZSTD y vuelve a empaquetar con LZ4",
    },
    naming: "Salida: se está generando el nombre",
    output: (name: string, dir: string) => (
      <>
        Salida: <b className="path">{name}</b> en <span className="path">{dir}</span>
      </>
    ),
    choices: { none: "Ninguno", zstd: "ZSTD", lz4: "LZ4" },
    levelLabel: (level: string, engine: string, n: number) => `${level}, ${engine} ${n}`,
    levelTip: (engine: string, n: number, note: string) => `${engine} ${n}. ${note}`,
    sumKeepZstd: "Los paquetes ZSTD se conservan byte a byte.",
    sumKeepLz4: "Los paquetes LZ4 se conservan byte a byte.",
    sumKeepPlain: "No se crean paquetes: los archivos del juego se convierten tal como están.",
    sumUnpack: (engine: string) => `Los paquetes ${engine} se vuelven a escribir como archivos normales.`,
    sumPack: (engine: string, level: string) => `Los archivos del juego se empaquetan en volúmenes ${engine} (${level}).`,
    sumRepack: (from: string, to: string, level: string) => `Los paquetes ${from} se decodifican y se vuelven a empaquetar con ${to} (${level}).`,
    sumRules: (name: string) => `Las reglas personalizadas ${name} eligen qué archivos se empaquetan.`,
    sumInside: (format: string, level: number) => `Dentro: ${format}, nivel de zlib ${level}.`,
    ngInstalled: "ampr-emu-ng se instala en fakelib/libSceAmpr.sprx.",
    ngReplaced: (old: string) => `El ${old} del juego se reemplaza por ampr-emu-ng.`,
    ngReplacedTrace: "El antiguo runtime de trazado del juego se reemplaza por ampr-emu-ng.",
    ngKept: (old: string) => `Se conserva el ${old} del juego.`,
    ngUpdated: (v: string) => `El ampr-emu-ng${v} del juego se reemplaza por el incluido.`,
    ngSame: (v: string) => `Se conserva el ampr-emu-ng${v} del juego.`,
    pkgNoRuntime: "El runtime de AMPR se omite del FPKG: lo proporciona el cargador de la consola.",
    recommended: "Recomendado",
    resultTitle: "Aproximadamente, tras la creación",
    resultNow: "Juego ahora",
    resultCustomRules: "reglas personalizadas: sin estimación",
    backportTitle: "Backport",
    backportNone: "Este juego no tiene backport.",
    backportPlain: "Solo en una conversión simple: sin empaquetar ni desempaquetar.",
  },
  // TransferPane.tsx: one device's pane of the Transfer screen
  transfer: {
    pane: (device) => `Archivos en ${device}`,
    roots: "Unidades",
    location: "Ubicación",
    up: "Subir",
    upTip: "Subir una carpeta",
    refresh: "Actualizar",
    filter: "Filtrar por nombre",
    filterLabel: "Filtrar esta carpeta por nombre",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "carpeta", many: "carpetas", other: "carpetas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => plural(n, { one: `${num(n)} coincidencia`, many: `${num(n)} coincidencias`, other: `${num(n)} coincidencias` }),
    firstOf: (max, n) => `primeros ${num(max)} de ${num(n)}`,
    truncated: "Esta carpeta contiene más de 10.000 elementos: solo se muestran los primeros 10.000.",
    name: "Nombre",
    size: "Tamaño",
    select: (name) => `Seleccionar ${name}`,
    open: (name) => `Abrir ${name}`,
    selected: (n) =>
      plural(n, { one: `${num(n)} elemento seleccionado`, many: `${num(n)} elementos seleccionados`, other: `${num(n)} elementos seleccionados` }),
    clear: "Quitar",
    clearTip: "Quitar la selección",
    dragItems: (n) => plural(n, { one: `${num(n)} elemento`, many: `${num(n)} elementos`, other: `${num(n)} elementos` }),
    dropHere: (path) => `Suelta para enviar a ${path}`,
    notHere: "Abre una carpeta para enviar archivos aquí.",
    emptyFolder: "Esta carpeta está vacía.",
    noMatch: (f) => `Nada de aquí coincide con “${f}”.`,
    noRoots: "No hay unidades disponibles para explorar.",
    cantList: "No se puede abrir esta carpeta:",
    loading: "Cargando",
    link: "Enlace",
    linkTip: "Un enlace: se puede abrir, pero no seleccionar",
    mounted: "Montado",
    mountedTip: "Montado por ShadowMount+: detén el juego y desactiva ShadowMount+ para seleccionarlo",
    withTitle: "Transferir con",
    loadingDevices: "Cargando dispositivos",
    noneTitle: "No hay ningún dispositivo con el que transferir",
    noneHint: "Vincula otro Dump Forge o añade un NAS en “Conectar”, o enciende uno guardado.",
    openConnect: "Abrir “Conectar”",
    deviceOffline: (device) => `${device} no tiene conexión. Sus archivos volverán a aparecer cuando vuelva.`,
    copyTo: (n, device) =>
      plural(n, {
        one: `Copiar ${num(n)} elemento en ${device}`,
        many: `Copiar ${num(n)} elementos en ${device}`,
        other: `Copiar ${num(n)} elementos en ${device}`,
      }),
    copyNoFolder: (device) => `Abre una carpeta en ${device} para copiar en ella.`,
    installOn: (device) => `Instalar en ${device}`,
    dropFiles: "Suelta archivos aquí para subirlos",
    uploadInto: (folder) => `Van a ${folder}, la carpeta que se muestra en este dispositivo.`,
    uploadNoFolder: "Abre una carpeta en este dispositivo para subir archivos a ella.",
    foldersLeftOut: (names) => `Las carpetas no se suben, solo los archivos: ${names}.`,
    downloadFiles: (n) =>
      plural(n, { one: `Descargar ${num(n)} archivo`, many: `Descargar ${num(n)} archivos`, other: `Descargar ${num(n)} archivos` }),
    downloadNone: "Selecciona archivos de este dispositivo para descargarlos (las carpetas no se descargan).",
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default es;
