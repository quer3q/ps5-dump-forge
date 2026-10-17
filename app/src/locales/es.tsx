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
    address: (url) => `Dirección: ${url}. Mostrar cómo conectarse, con un código QR`,
    screens: "Pantallas",
    convert: "Convertir",
    active: (n) => `, ${num(n)} ${plural(n, { one: "activa", many: "activas", other: "activas" })}`,
    edit: "Editar",
    library: "Biblioteca",
    about: "Acerca de",
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
    couldntStop: "No se pudo detener Dump Forge",
    close: "Cerrar",
    tryAgain: "Reintentar",
    stopped: "Dump Forge se ha detenido",
    startAgain: (ps5) => (ps5 ? "Para continuar, vuelve a abrir el icono de Dump Forge en la PS5." : "Para continuar, vuelve a ejecutar el mismo comando ps5-dump-forge serve."),
    confirmStop: (web: boolean) => (web ? "¿Detener Dump Forge?" : "¿Salir de PS5 Dump Forge?"),
    stopNote: (web, ps5) => (!web ? "No hay ningún trabajo en curso." : ps5 ? "Para volver a iniciarlo, abre el icono de Dump Forge en la PS5." : "Para volver a iniciarlo, ejecuta de nuevo el mismo comando ps5-dump-forge serve."),
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
      "Los recursos de este juego están empaquetados con LZ4 y su manifiesto LZ4 incluye el backport: primero desempaquétalo en la pestaña LZ4 y luego quita el backport aquí.",
    unreadable: "No se puede crear: no se pudo leer este origen (consulta “Origen”).",
    noContentId: "No se puede crear como fpkg: el param.json de este juego no tiene Content ID, y un paquete lo necesita.",
    blocked: (first, more) =>
      `No se puede crear: ${first}${more > 0 ? ` (y ${num(more)} más, consulta “Origen”)` : ""}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Este juego usa AMPR pero no tiene ampr_emu: la conversión instala el ampr_emu ${version} incluido.`;
      if (a === "trace") return `La conversión sustituye el runtime de trazado de LZ4 por el ampr_emu ${version} de lanzamiento incluido.`;
      if (a === "replace")
        return own
          ? `La conversión sustituye el ampr_emu ${own} de este juego por el ampr_emu ${version} incluido.`
          : `La conversión sustituye el ampr_emu de este juego, que no indica versión, por el ampr_emu ${version} incluido.`;
      return own
        ? `El ampr_emu ${own} de este juego es igual o más reciente que el ${version} incluido: la conversión lo conserva.`
        : `Este juego ya tiene el ampr_emu ${version} incluido: la conversión lo conserva.`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "ampr_emu.index no recoge los archivos de este juego" : state === "damaged" ? "No se puede leer ampr_emu.index" : "Este juego no tiene ampr_emu.index"}: la conversión escribe uno nuevo${traces ? " y deja fuera las trazas antiguas" : ""}.`,
    amprPkgIndex: (traces) =>
      `El paquete recibe un ampr_emu.index nuevo, generado a partir de sus archivos${traces ? ", sin las trazas antiguas" : ""}.`,
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
      ["Editar", "los archivos de un juego como una carpeta y ver su portada, firmware, backport, DLC y problemas de nombres de archivo antes de crear."],
      ["Eliminar", "un juego del disco, tras confirmarlo."],
      ["LZ4", "trazar un juego con AMPR y luego empaquetar sus recursos con reparación inteligente (experimental)."],
      ["En streaming", "de cualquier origen a cualquier destino: sin copia temporal, ni siquiera para la imagen dentro de un ffpfsc o un juego en un archivo comprimido."],
      ["Archivos comprimidos", "un .zip, .7z o .rar con un juego se explora, convierte o empaqueta como una carpeta."],
      ["Biblioteca", "los juegos de este ordenador o consola, listos para convertir y editar."],
      ["Seguro", "tu juego nunca cambia salvo que lo pidas; los archivos basura se omiten y los nombres de archivo incorrectos se avisan de antemano."],
      ["USB y tarjetas SD", "se escribe en memorias y tarjetas formateadas en exFAT o FAT32."],
      ["En el sitio o reconstruido", "los cambios van directo a la imagen si caben; si no, a una imagen nueva que la reemplaza tras comprobarla."],
    ],
    howToConnect: "Cómo conectarse",
    connectText: (url) => (
      <>Abre {url} en un navegador de cualquier dispositivo de tu red local, o escanea este código.</>
    ),
    close: "Cerrar",
    about: "Acerca de",
    aboutText: "Esta herramienta convierte un juego de cualquier formato a cualquier otro, en cualquier plataforma, incluida la PS5.",
    connectAny: "Cómo conectarse desde cualquier dispositivo",
    useAddress: (url) => <>Usa {url} o escanea este código.</>,
    compare: "Comparar formatos",
    compareLabel: "Comparación de formatos",
    format: "Formato",
    upstream: (app) =>
      `Cifras de sus autores: lo que indican los autores de ampr_emu y Lazy_AMPR, no medido por ${app}; varían según el juego. ampr_emu 0.4.2.1 es una versión de prueba de sus autores; problema conocido: algunos juegos se cierran al guardar.`,
    bundled: "Las carpetas empaquetadas con LZ4 funcionan con ampr_emu 0.4.2.1 de drakmor (GPL-3.0), incluido sin modificar. Código fuente:",
    lazy: "El flujo de trazar y luego empaquetar sigue Lazy_AMPR de Nazky:",
    unrar:
      "Los archivos .rar se leen con la biblioteca UnRAR incluida, de Alexander Roshal (solo extracción, bajo la licencia de UnRAR).",
    language: "Idioma",
    system: "Sistema",
  },
  // Edit.tsx
  edit: {
    details: "Detalles",
    deleting: "Eliminando el origen…",
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
      "Abre una **carpeta** o **imagen** de un juego y cámbiala como una carpeta en una memoria USB: **añade y reemplaza archivos** (arrástralos o usa “Añadir”), **cambia nombres, elimina**, crea carpetas.",
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
    items: (n) => `${num(n)} ${plural(n, { one: "elemento", other: "elementos" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "carpeta", other: "carpetas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "coincidencia", other: "coincidencias" })}`,
    emptyFolder: "Esta carpeta está vacía.",
    open: (name) => `Abrir ${name}`,
    archiveNote: "Los archivos comprimidos se pueden explorar, pero no editar. Para cambiarlo, conviértelo antes en una carpeta o una imagen.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "Añadir archivos",
    addFolder: "Añadir carpeta",
    pickAddFiles: "Elige los archivos que quieres añadir",
    pickAddFolder: "Elige la carpeta que quieres añadir",
    nameLabel: "Nombre",
    renameItem: (name) => `Cambiar el nombre de ${name}`,
    removeItem: (name) => `Eliminar ${name}`,
    renameTip: "Cambiar nombre (F2)",
    removeTip: "Eliminar (tecla Supr)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "Extraer",
    extractItem: (name: string) => `Extraer ${name}`,
    extractTip: "Copiar a una carpeta que elijas",
    extractFolder: "Extraer carpeta",
    extractFolderTip: "Copiar la carpeta mostrada a una carpeta que elijas",
    extractGameTip: "Copiar el juego completo a una carpeta que elijas",
    pickExtract: "Elige dónde extraer",
    extractPending: "Extrae los archivos tal como están ahora, sin tus cambios pendientes",
    nameEmpty: "Escribe un nombre.",
    nameBad: "Un nombre no puede contener / ni \\, ni ser . o ..",
    nameTaken: (name) => `${name} ya existe aquí.`,
    markNew: "Nuevo",
    markReplaced: "Reemplazado",
    markRenamed: "Renombrado",
    wasAt: (path) => `Antes: ${path}`,
    sizeLater: "Más de lo que se pudo listar: el resto se lee al aplicar los cambios",
    dropInto: (folder) => `Suelta para añadir a ${folder}`,
    pending: (n) =>
      plural(n, { one: `${num(n)} cambio sin aplicar todavía`, other: `${num(n)} cambios sin aplicar todavía` }),
    undo: "Deshacer",
    undoTip: "Deshacer el último cambio",
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
    lz4: {
      pack: "LZ4: empaquetar",
      unpack: "LZ4: desempaquetar",
      trace: "LZ4: trazado",
      unpatch: "LZ4: quitar parche",
      runtime: "Runtime AMPR",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "Edición",
    editRebuild: "Edición · reconstrucción",
    extract: "Extracción",
    job: (id) => `Tarea ${id}`,
    done: "Hecho",
    cancelled: "Cancelada",
    failed: "Error",
    cancelling: "Cancelando…",
    queued: "En cola",
    dismiss: "Descartar",
    stopJob: "¿Detener tarea?",
    cancel: "Cancelar",
    stopHint: "Vuelve a pulsar “¿Detener tarea?” en menos de 4 segundos para detenerla.",
    from: "de ",
    left: (d) => `tiempo restante: ${d}`,
    inStep: (time) => `${time} en este paso`,
    result: (bytes, n) => `${bytes}, ${num(n)} ${plural(n, FILES)} →`,
    outputPath: "Ruta de salida",
    checks: "Comprobaciones",
    verifiedPlain: (n) => `Verificado: ${num(n)} ${plural(n, CHECKS)} sin errores`,
    verifiedFull: (n) => `Verificación completa superada: ${num(n)} ${plural(n, CHECKS)}`,
    verifiedFast: (n) => `Verificación rápida superada: ${num(n)} ${plural(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `Cantidad revisada: ${checked} de ${total} en ${num(n)} ${plural(n, { one: "muestra", many: "muestras", other: "muestras" })}`,
    cantBuild: "No se puede crear este juego:",
    cantEdit: "No se pueden aplicar los cambios:",
    cantExtract: "No se puede extraer:",
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
    stays: "Los resultados se quedan aquí hasta que pulses “Actualizar”. Las salidas nuevas aparecen al actualizar.",
    library: "Biblioteca",
    storageTip: "Espacio libre en el almacenamiento interno de la consola (/data), según el último escaneo",
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
    lz4Tip: "Empaquetado con LZ4: paquetes de recursos de ampr_emu",
    lz4Packed: " empaquetado",
  },
  // Lz4.tsx
  lz4: {
    title: "Paquetes de recursos LZ4",
    help: "Cómo funciona LZ4",
    scenario: "Escenario LZ4",
    scenarios: { trace: "Trazado", pack: "Empaquetar/Desempaquetar" },
    actions: { patch: "Parchear", unpatch: "Quitar parche", pack: "Empaquetar", unpack: "Desempaquetar" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "Registra qué recursos lee el juego mientras juegas, para que Empaquetar los use."
      : packed
        ? "Vuelve a escribir los paquetes LZ4 del juego como archivos normales."
        : "Empaqueta los recursos del juego en volúmenes LZ4 que ampr_emu lee mientras el juego se ejecuta.",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "Vuelve a crear esta imagen con el runtime de trazado y luego la reemplaza."
        : "Instala el runtime de trazado en esta carpeta."
      : image
        ? "Vuelve a crear esta imagen con el runtime de lanzamiento y luego la reemplaza."
        : "Vuelve a poner el runtime de lanzamiento en esta carpeta.",
    noAmpr: "el juego no usa AMPR",
    noEboot: "no hay eboot.bin",
    packedFirst: "el origen está empaquetado con LZ4: desempaquétalo primero",
    damaged: "sus paquetes LZ4 están dañados",
    archive: "un archivo comprimido no se puede trazar: conviértelo primero",
    chooseFirst: "elige primero un origen",
    notRead: "el origen aún no se ha leído",
    pickProfile: "Perfil LZ4",
    profileFilter: "Perfil LZ4 (TOML)",
    pickTracesFolder: "Trazas LZ4 (una carpeta con ampr_commands.bin y ampr_emu.index)",
    pickTracesEither: "Trazas LZ4: el zip, o una carpeta con ambos archivos",
    pickTracesZip: "Trazas LZ4 (zip)",
    unreadable: "No se puede iniciar: no se pudo leer este origen (consulta “Origen”).",
    pending: (a) => a === "patch"
      ? "No se puede parchear mientras haya tareas en curso o en cola."
      : "No se puede quitar el parche mientras haya tareas en curso o en cola.",
    replacing: "Una tarea ya está volviendo a crear esta imagen: espera a que termine.",
    blocked: (a, first, more) => {
      const rest = `${first}${more > 0 ? ` (y ${num(more)} más, consulta “Origen”)` : ""}.`;
      if (a === "patch") return `No se puede parchear: ${rest}`;
      if (a === "unpatch") return `No se puede quitar el parche: ${rest}`;
      if (a === "pack") return `No se puede empaquetar: ${rest}`;
      return `No se puede desempaquetar: ${rest}`;
    },
    chooseGame: "Elige primero una carpeta o imagen de juego.",
    noAmprGame: "Este juego no usa AMPR, así que LZ4 no se puede usar aquí.",
    additionalContent: "Esto es contenido adicional (sin eboot.bin), así que LZ4 no se puede usar aquí.",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `Este juego no tiene ampr_emu: Empaquetar instala el ampr_emu ${version} incluido.`
        : a === "patch"
          ? `Este juego no tiene ampr_emu: Parchear instala el runtime de trazado incluido (ampr_emu ${version}).`
          : `Este juego no tiene ampr_emu: Quitar parche instala el runtime de lanzamiento incluido (ampr_emu ${version}).`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "ampr_emu.index no recoge los archivos de este juego" : state === "damaged" ? "No se puede leer ampr_emu.index" : "Este juego no tiene ampr_emu.index"}: ${a === "pack" ? "Empaquetar" : a === "patch" ? "Parchear" : "Quitar parche"} escribe uno nuevo.`,
    unavailable: (items) => `No disponible para este origen: ${items.join("; ")}.`,
    readOnly: (kind) => <>
      <b>{kind}</b> es de solo lectura en la consola, así que no se puede trazar. Conviértelo primero a <b>ffpkg</b> o <b>exfat</b>.
    </>,
    action: "Acción",
    traceAction: "Acción de trazado",
    changes: (a, image) => a === "patch"
      ? `Modifica ${image ? "esta imagen" : "esta carpeta"}: el runtime de trazado reemplaza fakelib/libSceAmpr.sprx, sin copia de seguridad.`
      : `Modifica ${image ? "esta imagen" : "esta carpeta"}: vuelve el runtime de lanzamiento y se eliminan las trazas, sin copia de seguridad.`,
    replay: (web) => `Juega y luego vuelve a elegir este origen para usar sus trazas.${web ? " “Trazado” también las ofrece como zip." : ""}`,
    room: "Espacio para la traza",
    roomHint: "Espacio libre para la traza del juego. Monta la imagen con image_rw= o no se registrará nada.",
    alreadyRelease: "Ya tiene el runtime de lanzamiento: solo se vuelve a escribir el índice.",
    spaceHint: "Necesita un espacio libre parecido al tamaño de la imagen; la imagen solo se reemplaza cuando la copia se verifica.",
    deletesTraces: (a, bytes, web) => a === "patch"
      ? `Parchear elimina las trazas de este origen (${bytes}): ${web ? "descárgalas antes." : "cópialas antes."}`
      : `Quitar el parche elimina las trazas de este origen (${bytes}): ${web ? "descárgalas antes." : "cópialas antes."}`,
    useProfile: "Usar un perfil de reglas en su lugar",
    profile: "Perfil",
    choose: "Elegir",
    profileHint: "Un archivo TOML con reglas de empaquetado, que se usa en lugar de las trazas.",
    target: "Destino",
    packInto: "Empaquetar en",
    direct: "Empaqueta directamente en la imagen; los archivos del juego se leen dos veces.",
    targetFormat: "Formato de destino",
    unpackFolder: "Una carpeta de juego nueva con los paquetes escritos de nuevo como archivos normales.",
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
    profileRules: "Empaqueta lo que indican las reglas del perfil; los archivos que el motor abre directamente quedan sueltos.",
    chosenRules: "Empaqueta los archivos que el juego leyó en estas trazas; los archivos que el motor abre directamente quedan sueltos.",
    ownRules: "Empaqueta los archivos que el juego leyó en su última sesión trazada; los archivos que el motor abre directamente quedan sueltos.",
    guessRules: "Sin trazas: empaqueta según una estimación integrada menos fiable. Traza primero, o elige trazas o un perfil.",
    recorded: "Trazas registradas",
    recordedHint: (bytes) => `ampr_commands.bin (${bytes}): Empaquetar usa estas trazas.`,
    downloadHint: "Cierra el juego, espera un minuto y descarga desde el navegador de una computadora. Allí elige el zip en “Empaquetar/Desempaquetar” → “Trazas”.",
    download: (bytes, zip) => `Descargar trazas (${bytes}) como ${zip}`,
    missingIndex: "Este origen no tiene ampr_emu.index: Empaquetar lo necesita junto a ampr_commands.bin.",
    truncatedJournal: "Un registro truncado al final también se empaqueta, hasta su última entrada completa.",
    fakelibWarning: (recorded) => recorded
      ? "¿Pocas trazas o ninguna? Desactiva el actualizador de fakelib, vuelve a parchear y juega de nuevo."
      : "Desactiva el actualizador de fakelib durante el trazado o no se registrará nada.",
    emptyTitle: "Elige un juego que use AMPR",
    experimental: "Experimental: guarda una copia del juego original.",
    chooseSource: "Elegir origen",
    source: "Origen",
    traces: "Trazas",
    optionalTraces: "Trazas (opcional)",
    unlock: "Desbloquear para elegir otras trazas",
    loaded: "Las trazas ya se cargaron de este origen",
    lockedHint: (bytes) => `Las propias trazas de este origen (${bytes}). Haz clic en el candado para elegir otras.`,
    clear: "Quitar",
    zip: "Zip",
    folder: "Carpeta",
    tracesHint: "Elige el zip amprtrace de “Descargar trazas”, o una carpeta con ambos archivos de traza.",
    useOwn: "Usar las trazas del origen",
    patchResult: (a, n) => a === "patch"
      ? `Parcheado: el runtime de trazado está instalado; el índice enumera ${num(n)} ${plural(n, FILES)}.`
      : `Parche quitado: vuelve el runtime de lanzamiento; el índice enumera ${num(n)} ${plural(n, FILES)}.`,
    removed: (files) => `${plural(files.length, {
  one: "Se eliminó el archivo de traza antiguo",
  many: "Se eliminaron los archivos de traza antiguos",
  other: "Se eliminaron los archivos de traza antiguos",
})}: ${files.join(", ")}.`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "Cómo funciona LZ4",
    close: "Cerrar",
    warning: "Experimental, solo para juegos con AMPR. Guarda una copia del juego original.",
    body: () => (
      <>
        <h3>1. Empaquetar/Desempaquetar</h3>
        <ol>
          <li>
            Elige el juego original, <b>Empaquetar/Desempaquetar</b> y las trazas (o un perfil).
          </li>
          <li>
            Crea una carpeta empaquetada o una imagen normal; una carpeta se convierte a cualquier formato en{" "}
            <b>Convertir</b>.
          </li>
          <li>
            Un origen empaquetado muestra <b>Desempaquetar</b> en su lugar: los archivos sueltos vuelven.
          </li>
          <li>Empaquetar conserva el ampr_emu del juego si es la versión incluida o una más nueva y lee paquetes LZ4; si no, instala el de lanzamiento incluido, también en lugar del runtime de trazado de LZ4 y en un juego que no lo tiene.</li>
        </ol>

        <h3>2. Trazado</h3>
        <ol>
          <li>
            Elige el <b>ffpkg</b> o el <b>exfat</b> del juego y luego <b>Trazado</b> → <b>Parchear</b>.
          </li>
          <li>Desactiva el actualizador de fakelib: sobrescribe el runtime de trazado.</li>
          <li>
            Móntalo con <span className="path">image_rw=</span>, juega una sesión y cierra el juego.
          </li>
          <li>
            Pulsa <b>Descargar trazas</b> en la página de la PS5, o copia ampr_commands.bin y ampr_emu.index.
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
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
    chooseFiles: (n) => `Elegir ${num(n)} ${plural(n, FILES)}`,
    nothing: "No hay carpetas ni archivos aquí.",
    cancel: "Cancelar",
    chooseFolder: "Elegir esta carpeta",
  },
  // common.tsx
  common: {
    folder: "Carpeta",
    pickFolder: "Carpeta del juego",
    pickImage: "Imagen o archivo comprimido del juego",
    pickImageFilter: "Imágenes y archivos comprimidos",
    pickFolderButton: "Carpeta",
    pickImageButton: "Imagen/Archivo comprimido",
    source: "Origen",
    emptyTitle: "Elige una carpeta, imagen o archivo comprimido de juego",
    emptyText: "Una carpeta de juego de PS5 o una imagen, paquete o archivo comprimido de ella.",
    accepted: "Orígenes admitidos",
    unknownTitle: "Título desconocido",
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
      forge_trace: "Forge de trazado",
      other: "otro",
      none: "ninguno",
    },
    packed: "Empaquetado",
    damaged: "Paquetes dañados",
    traced: "Trazado",
    plain: "Normal",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} en ${num(volumes)} ${plural(volumes, { one: "volumen", many: "volúmenes", other: "volúmenes" })}` +
      (percent === null ? "" : `, ${num(percent)}% del tamaño`),
    journal: (size) => `registro de ${size}`,
    noJournal: "aún sin registro",
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
        : "El archivo de imagen se eliminará para siempre: sin papelera ni forma de deshacerlo. Si la eliminación se detiene a medias, una parte puede haber desaparecido ya.",
    no: "No",
    yes: "Sí, eliminar",
    archiveTitle: "¿Eliminar este archivo comprimido?",
    archiveText:
      "El archivo comprimido se eliminará para siempre: sin papelera ni forma de deshacerlo. Las demás partes de un archivo en varias partes se quedan como están.",
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
        tone: "good",
        text: "Recomendado. El sistema de archivos propio de la PS5 (UFS2). Admite montajes con escritura.",
      },
      ffpfs: {
        tone: "warn",
        text: "Experimental. El sistema de archivos PFS de la PS5, sin comprimir. Los nombres de archivo deben ser ASCII simple.",
      },
      ffpfsc: {
        tone: "warn",
        text: "Experimental. El más pequeño: un contenedor comprimido alrededor de una imagen, siempre montado en solo lectura.",
      },
      pkg: {
        text: "Se instala como un juego de la tienda y funciona a velocidad nativa. Necesita kstuff, fpkg-enable y ppr-patch.",
      },
      lz4: {
        tone: "warn",
        text: "Experimental. Una carpeta de juego con sus recursos empaquetados en volúmenes LZ4, que el emulador AMPR incluido (ampr_emu) desempaqueta mientras el juego se ejecuta. Solo para juegos que usan AMPR.",
      },
    },
    innerFfpkg: { tone: "good", text: "Recomendado. El sistema de archivos propio de la PS5 (UFS2)." },
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
    pkgFirmware: "Solo funciona con firmware 11.60 o inferior.",
    imageInside: "Imagen interior",
    imageInsideLabel: "Imagen dentro del ffpfsc",
    compressionLevel: "Nivel de compresión",
    compression: "Compresión",
    fullVerify: "Verificación completa",
    fullVerifyHint: "Vuelve a leer cada byte para comprobar la salida. Tarda más.",
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
    updatingNote: "La nueva versión se está descargando, la app se reiniciará sola.",
    downloading: "Descargando en tu navegador: descomprime el archivo y reemplaza esta copia con su contenido.",
    failed: (v) => `No se pudo actualizar a la v${v}`,
    timeout: (v) => `La v${v} no se inició en un minuto. Si se descargó, detén Dump Forge y vuelve a abrirlo para terminar la actualización.`,
    retry: "Reintentar",
    close: "Cerrar",
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default es;
