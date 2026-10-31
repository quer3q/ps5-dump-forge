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
    convert: "Convertir",
    active: (n) => `, ${num(n)} ${plural(n, { one: "activa", many: "activas", other: "activas" })}`,
    edit: "Editar",
    pack: "Empaquetar",
    storage: "Almacenamiento",
    connect: "Conectar",
    disconnected: "Desconectado",
    connected: (n: number) => plural(n, { one: `Conectado (${num(n)} dispositivo)`, many: `Conectado (${num(n)} dispositivos)`, other: `Conectado (${num(n)} dispositivos)` }),
    about: "Acerca de",
    prevTab: "Pestaña anterior (L2)",
    nextTab: "Pestaña siguiente (R2)",
    offline: "Sin conexión, reintentando…",
    stop: "Detener Dump Forge",
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
  // Convert.tsx
  convert: {
    target: "Destino",
    targetFormat: "Formato de destino",
    alreadyFolder: "el origen ya es una carpeta",
    lz4Backport:
      "Los recursos de este juego están empaquetados con LZ4 y su manifiesto LZ4 incluye el backport: primero desempaquétalo en la pestaña Empaquetar y luego quita el backport aquí.",
    zstdBackport:
      "Los recursos de este juego están empaquetados con zstd y su índice zstd incluye el backport: primero desempaquétalo en la pestaña Empaquetar y luego quita el backport aquí.",
    unreadable: "No se puede crear: no se pudo leer este origen (consulta “Origen”).",
    patchBlocked: "No se puede crear: es un paquete de parche, que solo contiene lo que cambió la actualización. Ábrelo en “Editar” para extraer sus archivos.",
    openInEdit: "Abrir en “Editar”",
    noContentId: "No se puede crear como fpkg: el param.json de este juego no tiene Content ID, y un paquete lo necesita.",
    blocked: (first, more) =>
      `No se puede crear: ${first}${more > 0 ? ` (y ${num(more)} más, consulta “Origen”)` : ""}.`,
    amprZstdRuntime: "Esta conversión instala ampr-emu-ng en fakelib/libSceAmpr.sprx, que estos paquetes necesitan.",
    amprNgReplaced: (version) => `Reemplaza el ampr-emu-ng de este juego por ampr_emu ${version}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Este juego usa AMPR pero no tiene ampr_emu: la conversión instala el ampr_emu ${version} incluido.`;
      if (a === "trace") return `La conversión sustituye el ampr_emu de este juego por el ampr_emu ${version} incluido.`;
      if (a === "replace")
        return own
          ? `La conversión sustituye el ampr_emu ${own} de este juego por el ampr_emu ${version} incluido.`
          : `La conversión sustituye el ampr_emu de este juego, que no indica versión, por el ampr_emu ${version} incluido.`;
      return own
        ? `El ampr_emu ${own} de este juego es igual o más reciente que el ${version} incluido: la conversión lo conserva.`
        : `Este juego ya tiene el ampr_emu ${version} incluido: la conversión lo conserva.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index no recoge los archivos de este juego" : state === "damaged" ? "No se puede leer ampr_emu.index" : "Este juego no tiene ampr_emu.index"}: la conversión escribe uno nuevo.`,
    amprPkgIndex: "El paquete recibe un ampr_emu.index nuevo, generado a partir de sus archivos.",
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
      zstd: { tagline: "Para juegos con AMPR, con ampr-emu-ng", cells: ["Carga hasta un 80 % más rápida", "Hasta la mitad del tamaño del juego"] },
      lz4: {
        tagline: "Experimental, para juegos que usan AMPR",
        cells: ["Carga entre 30 y 40% más rápida, según sus autores", "Más o menos la mitad del juego, según sus autores"],
      },
      ffpkg: { tagline: "Recomendado para ShadowMountPlus", cells: ["Velocidad completa de imagen", FLAT] },
      exfat: { tagline: "Compatibilidad", cells: ["Velocidad completa de imagen", FLAT] },
      ffpfs: { tagline: "Experimental", cells: ["Velocidad completa de imagen", FLAT] },
      ffpfsc: {
        tagline: "El más pequeño, experimental",
        cells: ["150–250 MB/s, puede dar tirones", "Depende del nivel de compresión, normalmente entre 30 y 60% menos"],
      },
      folder: { tagline: "Archivos del juego sin más", cells: ["Velocidad completa de la unidad", FLAT] },
    },
    features: [
      ["Convertir", "entre carpeta, ffpkg, exfat, ffpfs, ffpfsc y fpkg, en ambos sentidos."],
      ["Transferir", "archivos y convertir juegos entre tu PS5, ordenadores y un NAS por SMB, FTP o NFS."],
      ["Editar", "los archivos de un juego como una carpeta y ver su portada, firmware, backport, DLC y problemas de nombres de archivo antes de crear."],
      ["En streaming", "de cualquier origen a cualquier destino: sin copia temporal, ni siquiera para la imagen dentro de un ffpfsc o un juego en un archivo comprimido."],
      ["Archivos comprimidos", "un .zip, .7z o .rar con un juego se explora, convierte o empaqueta como una carpeta."],
      ["USB y tarjetas SD", "se escribe en memorias y tarjetas formateadas en exFAT o FAT32."],
      ["Empaquetar", "paquetes de recursos zstd o LZ4 (experimental) con reparación inteligente."],
      ["Almacenamiento", "una Biblioteca de tus juegos y un Explorador de archivos para renombrar, mover y eliminar."],
      ["Seguro", "tu juego nunca cambia salvo que lo pidas; los archivos basura se omiten y los nombres de archivo incorrectos se avisan de antemano."],
    ],
    about: "Acerca de",
    compare: "Comparar formatos",
    compareLabel: "Comparación de formatos",
    format: "Formato",
    credits: "Créditos",
    upstream: (app) =>
      `Cifras de sus autores: lo que indican los autores de ampr_emu y Lazy_AMPR, no medido por ${app}; varían según el juego. ampr_emu 0.4.2.1 es una versión de prueba de sus autores; problema conocido: algunos juegos se cierran al guardar.`,
    bundled: "Las carpetas empaquetadas con LZ4 funcionan con ampr_emu 0.4.2.1 de drakmor (GPL-3.0), incluido sin modificar. Código fuente:",
    lazy: "El empaquetado LZ4 se basa en Lazy_AMPR de Nazky:",
    ng: "Los paquetes zstd y LZ4 funcionan con ampr-emu-ng. Código fuente:",
    logo: "El logotipo es obra de Xxcco:",
    unrar:
      "Los archivos .rar se leen con la biblioteca UnRAR incluida, de Alexander Roshal (solo extracción, bajo la licencia de UnRAR).",
    language: "Idioma",
    system: "Sistema",
    notifications: "Avisar al terminar una tarea",
    notificationsHint: "Se muestra en la consola para tareas de conversión, empaquetado y transferencia.",
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
    /** Edit's folds under the game card. */
    facts: "Datos del juego",
    explorer: "Archivos del juego",
    howTitle: "Cómo funciona",
    /** The empty state's paragraphs. */
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
    archiveNote: "Los archivos comprimidos se pueden explorar, pero no editar. Para cambiarlo, conviértelo antes en una carpeta o una imagen.",
    nasNote: "Los juegos de un NAS se pueden explorar y extraer, pero no editar. Para cambiarlo, conviértelo o extráelo primero a este dispositivo.",
    patchNote: "Es un paquete de parche: solo contiene lo que cambió la actualización. Se muestran los archivos que contiene completos; se pueden explorar y extraer, pero no editar.",
    peerNote: "Los juegos de otro Forge se pueden explorar y extraer, pero no editar. Para cambiarlo, conviértelo o extráelo primero a este dispositivo.",
    /** The explorer's changes: none is made until Apply. */
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
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
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
    /** How the changes go in (the pending bar), from core's check before Apply. */
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
    alwaysRebuilt: "Las imágenes ffpfsc y fpkg siempre se reconstruyen.",
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
    applyRunning: "Aplicando los cambios. La edición espera a que termine la tarea de abajo.",
    applyFailed: "No se aplicaron los cambios:",
    applyRefused: "Los cambios no se pueden aplicar de esta forma:",
    applyCancelled: "La tarea se canceló. Tus cambios siguen en la lista.",
    folderPartly: "Puede que una parte ya esté en la carpeta: los archivos que se muestran son lo que contiene ahora.",
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
      zstdPack: (level: string) => `zstd: empaquetar · ${level}`,
      level: (n: number) => `nivel ${num(n)}`,
      zstdUnpack: "zstd: desempaquetar",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
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
    result: (bytes, n) => `${bytes}, ${num(n)} ${plural(n, FILES)} →`,
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
  // Library.tsx
  library: {
    pickTitle: "Elige una carpeta para la Biblioteca",
    scanningIn: (name) => `Escaneando ${name}`,
    scanning: "Escaneando",
    cantScanFolder: "No se puede escanear esta carpeta",
    chooseAnother: "Elegir otra carpeta",
    pickPrompt: "Elige la carpeta de tu biblioteca para escanearla",
    choose: "Elegir carpeta",
    cantScan: "No se puede escanear la Biblioteca:",
    cantDelete: "No se puede eliminar:",
    deleted: (path) => `Eliminado ${path}`,
    truncated: "Se alcanzó el límite de escaneo. Puede que la lista esté incompleta.",
    limits: "La búsqueda llega hasta seis niveles de carpetas y se detiene tras unos 15 segundos de escaneo.",
    noRoots: "No hay carpetas disponibles para escanear.",
    none: "No se encontraron juegos. La búsqueda llega hasta seis niveles de profundidad y omite las carpetas del sistema. Puedes elegir otros orígenes en “Editar” o “Convertir”.",
    skipped: "Ubicaciones y archivos omitidos",
    library: "Biblioteca",
    storageTip: "Espacio libre en el almacenamiento interno de la consola (/data)",
    free: (free, total) => (
      <>
        Libre: <b>{free}</b> de {total}
      </>
    ),
    refresh: "Actualizar",
    cards: "Vista de tarjetas",
    list: "Vista de lista",
    games: "Juegos",
    edit: (n) => `Editar ${n}`,
    convert: (n) => `Convertir ${n}`,
    remove: (n) => `Eliminar ${n}`,
    deleting: (n) => `Eliminando ${n}`,
    lz4Tip: "Paquetes de recursos LZ4",
    zstdTip: "Paquetes de recursos zstd",
    showAs: "Mostrar como",
    kinds: "Mostrar tipo",
    all: "Todos",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n, path) => (
      <>
        <b>{num(n)}</b> {plural(n, { one: "juego en", many: "juegos en", other: "juegos en" })} {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "Vista de almacenamiento",
    library: "Biblioteca",
    files: "Explorador de archivos",
    title: "Explorador de archivos",
    pickTitle: "Elige una carpeta para explorar",
    pickPrompt: "Elige una carpeta para explorar sus archivos",
    choose: "Elegir carpeta",
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
    otherDevice: "“Mover” no sale de un dispositivo: arrástralo a la pestaña de otro dispositivo para copiarlo allí.",
    dropCopy: (folder) => `Suelta para copiar en ${folder}`,
    dropMove: (folder) => `Suelta para mover a ${folder}`,
    cantCopy: "No se puede copiar:",
    copyNoFolder: "Abre primero una carpeta en ese dispositivo.",
    copyOne: (name, to) => `Copiando ${name} en ${to}. La transferencia aparece en “Conectar”.`,
    copyMany: (n, to) =>
      plural(n, {
        one: `Copiando ${num(n)} elemento en ${to}. La transferencia aparece en “Conectar”.`,
        many: `Copiando ${num(n)} de elementos en ${to}. La transferencia aparece en “Conectar”.`,
        other: `Copiando ${num(n)} elementos en ${to}. La transferencia aparece en “Conectar”.`,
      }),
    openConnect: "Abrir “Conectar”",
  },
  // Pack.tsx
  pack: {
    title: "Empaquetar",
    help: "Cómo funciona empaquetar",
    engine: "Tipo de paquetes",
    engines: { zstd: "zstd", lz4: "LZ4" },
    actions: { pack: "Empaquetar", unpack: "Desempaquetar" },
    info: (e, packed) =>
      e === "zstd"
        ? packed
          ? "Vuelve a escribir los paquetes zstd como archivos normales."
          : "Empaqueta los recursos del juego en volúmenes zstd."
        : packed
          ? "Vuelve a escribir los paquetes LZ4 como archivos normales."
          : "Puede que algunos juegos no arranquen. Empaqueta los recursos del juego en volúmenes LZ4.",
    noAmprGame: (e) => `Este juego no usa AMPR, así que los paquetes ${e === "zstd" ? "zstd" : "LZ4"} no se pueden usar.`,
    patch: "Un paquete de parche no se puede empaquetar.",
    additionalContent: "El contenido adicional (sin eboot.bin) no se puede empaquetar.",
    unreadable: "No se puede iniciar: no se pudo leer el origen (consulta “Origen”).",
    blocked: (a, first, more) =>
      `No se puede ${a === "pack" ? "empaquetar" : "desempaquetar"}: ${first}${more > 0 ? ` (y ${num(more)} más, consulta “Origen”)` : ""}.`,
    target: "Destino",
    packFolder: (e) => `Recomendado. Lo más rápido, la menor latencia: una carpeta de juego nueva, con sus recursos en paquetes ${e === "zstd" ? "zstd" : "LZ4"}.`,
    levels: { fast: "Rápido", medium: "Medio", max: "Máximo" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Recomendado. zstd 5: rápido, buen tamaño." },
        medium: { text: "zstd 9: más pequeño, más lento." },
        max: { text: "zstd 19: el más pequeño; mucho más lento, más memoria." },
      },
      lz4: {
        fast: { tone: "good", text: "Recomendado. LZ4 por defecto: el más rápido." },
        medium: { text: "LZ4 HC 9: más pequeño, más lento." },
        max: { text: "LZ4 HC 12: el más pequeño, el más lento." },
      },
    },
    runtime: "Runtime",
    runtimeNotes: {
      ng: { tone: "good", text: "Recomendado. Lee paquetes zstd y LZ4." },
      ampr_emu: { text: "El ampr_emu original." },
    },
    ngNote: "Necesita ampr-emu-ng: Empaquetar lo instala en fakelib/. Desactiva la actualización de fakelib de ShadowMount+, o reemplazará el runtime.",
    installsRuntime: (version) => `Aquí no hay ampr_emu: Empaquetar instala ampr_emu ${version}.`,
    freshIndex: (state) =>
      `${state === "stale" ? "ampr_emu.index está desactualizado" : state === "damaged" ? "ampr_emu.index está dañado" : "No hay ampr_emu.index"}: Empaquetar escribe uno nuevo.`,
    direct: "Empaqueta directamente en la imagen; lee el juego dos veces.",
    unpackFolder: "Una carpeta de juego nueva con archivos normales.",
    unpackHint: (n) =>
      n === null
        ? "Vuelve a escribir los archivos empaquetados como archivos normales."
        : `Vuelve a escribir ${plural(n, { one: `${num(n)} archivo empaquetado como archivo normal`, many: `${num(n)} archivos empaquetados como archivos normales`, other: `${num(n)} archivos empaquetados como archivos normales` })}.`,
    likelyFail: (fmt) => <>
      Probablemente fallará como <b>{fmt}</b>: no admite los nombres de archivo indicados en Origen.
    </>,
    collides: (a, folder) => {
      const what = folder ? "esta carpeta" : "este archivo";
      return a === "pack"
        ? `Una tarea ya está escribiendo ${what}: Empaquetar necesita otro nombre.`
        : `Una tarea ya está escribiendo ${what}: Desempaquetar necesita otro nombre.`;
    },
    already: (a, fmt) => a === "pack"
      ? <>Este juego ya se está escribiendo como <b>{fmt}</b>; volver a empaquetar hace una segunda copia.</>
      : <>Este juego ya se está escribiendo como <b>{fmt}</b>; volver a desempaquetar hace una segunda copia.</>,
    source: "Origen",
  },
  // PackHelp.tsx
  packHelp: {
    title: "Cómo funciona empaquetar",
    close: "Cerrar",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>Empaqueta los recursos del juego en volúmenes zstd, en una carpeta o una imagen. Solo para juegos que usan AMPR.</li>
          <li><b>Rápido</b> (zstd 5) es veloz; <b>Medio</b> (9) y <b>Máximo</b> (19) son más pequeños y más lentos.</li>
          <li>Un juego empaquetado muestra <b>Desempaquetar</b>: vuelven los archivos normales.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Puede que algunos juegos no arranquen.</b> Guarda una copia.</li>
          <li>Empaqueta en volúmenes LZ4, en una carpeta o una imagen: <b>Rápido</b> (LZ4), <b>Medio</b> (HC 9), <b>Máximo</b> (HC 12).</li>
          <li>Runtime: <b>ampr-emu-ng</b> (por defecto) o el <b>ampr_emu</b> original.</li>
          <li>Un juego empaquetado muestra <b>Desempaquetar</b>.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>Lee paquetes zstd y LZ4; Empaquetar lo instala en fakelib/.</li>
          <li>Desactiva la actualización de fakelib de ShadowMount+, o reemplazará el runtime y el juego no podrá leer sus paquetes.</li>
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
    runtime: {
      forge_release: "Forge de lanzamiento",
      forge_trace: "runtime de trazado LZ4 antiguo",
      zstd: "ampr-emu-ng",
      other: "otro",
      none: "ninguno",
    },
    packed: "Empaquetado",
    zstdKept: "paquetes de recursos zstd: la conversión los conserva",
    damaged: "Paquetes dañados",
    plain: "Normal",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} en ${num(volumes)} ${plural(volumes, { one: "volumen", many: "volúmenes", other: "volúmenes" })}` +
      (percent === null ? "" : `, ${num(percent)}% del tamaño`),
    runtimeIs: (label) => `runtime: ${label}`,
    index: {
      stale: "ampr_emu.index desactualizado: una conversión lo renueva",
      damaged: "ampr_emu.index dañado: una conversión lo reescribe",
      missing: "sin ampr_emu.index: una conversión lo escribe",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `una conversión instala el ampr_emu ${version} incluido`
        : a === "replace"
          ? `una conversión sustituye el runtime por el ampr_emu ${version} incluido`
          : `ampr_emu ${version}: una conversión lo conserva`,
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
    wrong: "La contraseña no es correcta.",
    label: "Contraseña",
    cancel: "Cancelar",
    unlock: "Desbloquear",
    checking: "Comprobando…",
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
    format: {
      folder: { text: "Desempaqueta una imagen o un paquete de vuelta a una carpeta de juego normal." },
      exfat: {
        text: "Para juegos que fallan como ffpkg y solo funcionan como contenido de una unidad externa. También se abre en un Mac o PC.",
      },
      ffpkg: {
        text: "El sistema de archivos propio de la PS5 (UFS2). Admite montajes con escritura.",
      },
      ffpfs: {
        text: "El sistema de archivos PFS de la PS5, sin comprimir. Los nombres de archivo deben ser ASCII simple.",
      },
      ffpfsc: {
        text: "El más pequeño: un contenedor comprimido alrededor de una imagen, siempre montado en solo lectura.",
      },
      pkg: { tone: "good", text: "Recomendado. Funciona como un juego nativo, pero solo si tu firmware lo permite." },
      lz4: {
        tone: "warn",
        text: "Experimental. Una carpeta de juego con sus recursos empaquetados en volúmenes LZ4, que el emulador AMPR incluido (ampr_emu) desempaqueta mientras el juego se ejecuta. Solo para juegos que usan AMPR.",
      },
    },
    innerFfpkg: { text: "El sistema de archivos propio de la PS5 (UFS2)." },
    innerExfat: { tone: "good", text: "Recomendado. La imagen más estable dentro de un contenedor comprimido." },
    level0: { text: "Sin compresión: cada bloque se guarda tal cual. El archivo ocupa lo mismo que la imagen." },
    levelDefault: {
      tone: "good",
      text: "Recomendado: el valor por defecto de zlib. El nivel 1 crea unas 3 veces más rápido con un archivo un 3% más grande; el 9 apenas reduce el tamaño.",
    },
    levelLow: {
      text: "Crea más rápido que el 6 (el nivel 1, unas 3 veces); el archivo sale más grande (el nivel 1, un 3%).",
    },
    levelHigh: {
      text: "Apenas más pequeño que el 6 (el nivel 9, un 0,04%), pero comprimir tarda más (el nivel 9, un 25%), con todos los núcleos ocupados todo el tiempo.",
    },
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
    pkgExt: (ext) => `Termina en .${ext}, pero fpkg se guarda como .pkg: la extensión determina el controlador.`,
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
    generatePattern: "[nombre del juego]-[ID del título]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} está disponible`,
    update: "Actualizar",
    later: "Más tarde",
    updating: "Actualizando…",
    updatingNote: "La nueva versión se está descargando. Esta página se recargará sola con ella.",
    downloading: "Descargando en tu navegador: descomprime el archivo y reemplaza esta copia con su contenido.",
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
    details: (name) => `Detalles de ${name}`,
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
    pickSendFiles: "Archivos que enviar",
    pickSendFolder: "Carpeta que enviar",
    pickSaveTo: "Dónde guardarlo",
    thisBrowser: "Este navegador",
    browserHint: "Los archivos de este teléfono u ordenador van a una carpeta de este dispositivo, y este navegador guarda un archivo de este dispositivo.",
    uploadTo: "Carpeta de destino",
    uploadPick: "Elegir archivos para subir",
    uploading: (name, i, n) => `Subiendo ${name} (${num(i)} de ${num(n)})`,
    uploaded: "Subida terminada",
    uploadSkipped: (names) => `No se subió, allí hay una carpeta o un enlace con ese nombre: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `Se subió ${num(done)} de ${num(n)} archivos y se queda en este dispositivo.`,
        other: `Se subieron ${num(done)} de ${num(n)} archivos y se quedan en este dispositivo.`,
      }),
    cantStart: "No se puede iniciar la transferencia:",
    cantUpload: "No se puede subir:",
    pickDownload: "Archivo que descargar",
    transferIntro: "Envía archivos o una carpeta desde cualquier dispositivo a una carpeta de aquí, de un dispositivo vinculado o de un NAS. La transferencia se ejecuta como una tarea en la lista de tareas.",
    send: "Enviar",
    sendWhat: "Qué enviar",
    sendTo: "Enviar a",
    sendNeedsDevice: "Conecta primero otro dispositivo: vincula un Forge o añade un NAS.",
    nothingChosen: "Aún no se ha elegido nada",
    chooseFiles: "Elegir archivos",
    chooseFolder: "Elegir una carpeta",
    startTransfer: "Iniciar",
    uploadHere: "Subir a este dispositivo",
    downloadHere: "Descargar de este dispositivo",
    consoleNoFiles: "El navegador de la consola no puede elegir archivos: abre esta página en un teléfono o un ordenador.",
    wholeRoot: "No se puede enviar un dispositivo, recurso compartido o unidad entero: elige una carpeta dentro.",
    helpPhone: "Escanea el código QR para abrir esta página en un teléfono. Allí, Subir a este dispositivo y Descargar de este dispositivo en Transferencia de archivos mueven los archivos del teléfono.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "Conectar une Dump Forge de tu PS5 y tus ordenadores en la misma red.",
      "Para vincular, inicia la Detección automática y elige Vincular en Cerca, o escribe una dirección en Añadir un dispositivo, y permítelo en el otro dispositivo. Permítelo solo si ambas pantallas muestran el mismo número de comprobación.",
      "Un NAS (SMB, FTP, NFS) se añade igual. Los dispositivos vinculados y los NAS sirven como origen de juegos y como destino; sus juegos no se editan allí.",
      "Transferencia de archivos copia archivos y carpetas entre cualquiera de ellos. Un nombre que ya existe allí solo se reemplaza después de que lo confirmes.",
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
    started: (name) => `Instalando ${name}. La tarea aparece abajo.`,
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
    sent: (device) => `${device} está instalando el paquete. La tarea aparece abajo.`,
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default es;
