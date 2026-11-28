// Brazilian Portuguese. Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Terms: rastrear / rastreamento / rastreamentos (trace), empacotar / desempacotar (pack),
// aplicar patch / remover patch (patch), runtime (o runtime).

import { dotted, num, plural } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "Igual ao tamanho do jogo";
const FILES = { one: "arquivo", many: "arquivos", other: "arquivos" };
const CHECKS = { one: "verificação", many: "verificações", other: "verificações" };

const pt: Messages = {
  // App.tsx
  app: {
    screens: "Telas",
    active: (n) => `, ${num(n)} ${plural(n, { one: "ativa", many: "ativas", other: "ativas" })}`,
    edit: "Editar",
    storage: "Armazenamento",
    connect: "Conectar",
    disconnected: "Desconectado",
    connected: (n: number) =>
      plural(n, { one: `${num(n)} dispositivo`, many: `${num(n)} dispositivos`, other: `${num(n)} dispositivos` }),
    about: "Sobre",
    offline: "Sem conexão, tentando de novo…",
    stop: "Parar",
    stillRunning: (n) =>
      n === 1 ? "Uma tarefa ainda está em execução" : `${num(n)} tarefas ainda estão em execução`,
    job: (id) => `Tarefa ${id}`,
    quitNote: (web, n) =>
      web
        ? n === 1
          ? "Parar o Dump Forge interrompe a tarefa e exclui a saída incompleta."
          : "Parar o Dump Forge interrompe as tarefas e exclui a saída incompleta."
        : n === 1
          ? "Sair interrompe a tarefa e exclui a saída incompleta."
          : "Sair interrompe as tarefas e exclui a saída incompleta.",
    keepRunning: "Continuar",
    stopAndQuit: (n) => (n === 1 ? "Parar a tarefa e sair" : "Parar as tarefas e sair"),
    stoppingJobs: "Parando as tarefas e limpando…",
    stoppingForge: "Parando o Dump Forge…",
    quitting: "Saindo do PS5 Dump Forge…",
    couldntStop: "Não foi possível parar o Dump Forge",
    close: "Fechar",
    tryAgain: "Tentar de novo",
    stopped: "O Dump Forge foi parado",
    startAgain: (ps5) => (ps5 ? "Para continuar, abra de novo o bloco do Dump Forge no PS5." : "Para continuar, execute de novo o mesmo comando ps5-dump-forge serve."),
    confirmStop: (web: boolean) => (web ? "Parar o Dump Forge?" : "Sair do PS5 Dump Forge?"),
    stopNote: (web, ps5) => (!web ? "Nenhuma tarefa em andamento." : ps5 ? "Para iniciá-lo de novo, abra o bloco do Dump Forge no PS5." : "Para iniciá-lo de novo, execute outra vez o mesmo comando ps5-dump-forge serve."),
    peerBusy: "Um dispositivo pareado está lendo ou gravando arquivos neste computador. Sair interrompe a transferência dele.",
    quit: "Sair",
    github: (name) => `${name} no GitHub`,
    queued: "na fila",
    progress: (percent, left) => `${percent}% concluído${left ? `, tempo restante: ${left}` : ""}`,
  },
  // build/ (Build's convert settings and checks)
  convert: {
    targetFormat: "Formato",
    alreadyFolder: "a origem já é uma pasta",
    patchBlocked: "Não é possível criar: este é um pacote de patch, que contém só o que a atualização mudou. Abra-o em “Editar” para extrair os arquivos.",
    openInEdit: "Abrir em “Editar”",
    noContentId: "Não é possível criar como FPKG: o param.json deste jogo não tem Content ID, que um pacote exige.",
    blocked: (first, more) =>
      `Não é possível criar: ${first}${more > 0 ? ` (e mais ${num(more)}, veja “Origem”)` : ""}.`,
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "O ampr_emu.index não lista os arquivos deste jogo" : state === "damaged" ? "Não dá para ler o ampr_emu.index" : "Este jogo não tem ampr_emu.index"}: a conversão grava um novo.`,
    removeBackport: "Remover backport",
    removeHint: (libs, emulators, other) =>
      `Omite de fakelib/ ${num(libs)} ${plural(libs, { one: "biblioteca", many: "bibliotecas", other: "bibliotecas" })} de backport` +
      (emulators.length === 0
        ? other
          ? "; as outras bibliotecas homebrew permanecem."
          : "."
        : `; ${emulators.length === 1 ? "o emulador" : "os emuladores"} (${emulators.join(", ")})${other ? " e as outras bibliotecas homebrew" : ""} ${emulators.length === 1 && !other ? "permanece" : "permanecem"}.`),
    cantRemove: (why) => `Não é possível remover o backport: ${why}`,
    compare: "Comparar formatos",
    build: "Criar",
    likelyFail: (fmt) => (
      <>
        Provavelmente vai falhar como <b>{fmt}</b>: esse formato recusa os nomes de arquivo listados em “Origem”.
      </>
    ),
    collides: "Uma tarefa já está gravando este arquivo: clicar em “Criar” de novo vai falhar. Escolha outro nome.",
    already: (fmt) => (
      <>
        Este jogo já está sendo criado como <b>{fmt}</b>; clicar em “Criar” de novo gera uma segunda cópia.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "Bom", ok: "Meio-termo", bad: "Desvantagem" },
    columns: ["Velocidade no console", "Tamanho em disco"],
    rows: {
      pkg: {
        tagline: "O melhor, se o firmware permitir",
        cells: ["Nativa, velocidade total", "30–60% menor, dependendo do jogo"],
      },
      zstd: { tagline: "Com ampr-emu-ng", cells: ["Carregamento mais rápido", "Até metade do tamanho do jogo"] },
      lz4: {
        tagline: "Com ampr-emu-ng",
        cells: ["Carregamento 30–40% mais rápido, segundo os autores", "Cerca de metade do tamanho do jogo, segundo os autores"],
      },
      ffpkg: { tagline: "Recomendado para o ShadowMountPlus", cells: ["Velocidade total de imagem", FLAT] },
      exfat: { tagline: "Compatibilidade", cells: ["Velocidade total de imagem", FLAT] },
      ffpfs: { tagline: "PFS sem compressão", cells: ["Velocidade total de imagem", FLAT] },
      ffpfsc: {
        tagline: "O menor",
        cells: ["150–250 MB/s, pode engasgar", "Depende do nível de compressão, geralmente 30–60% menor"],
      },
      folder: { tagline: "Arquivos comuns do jogo", cells: ["Velocidade total da unidade", FLAT] },
    },
    compare: "Comparar formatos",
    compareLabel: "Comparação de formatos",
    format: "Formato",
    credits: "Créditos",
    upstream: (app: string) =>
      `Números dos autores: o que os autores do ampr_emu e do Lazy_AMPR relatam, sem medição do ${app}; variam de jogo para jogo.`,
    bundled: "ampr_emu de drakmor (GPL-3.0), do qual o ampr-emu-ng é um fork. O Forge não o inclui mais: troca o ampr_emu de um jogo pelo ampr-emu-ng, a menos que as Configurações o mantenham. Código-fonte:",
    lazy: "A estratégia de empacotamento se baseia no Lazy_AMPR de Nazky:",
    ng: "Os pacotes ZSTD e LZ4 rodam no ampr-emu-ng (GPL-3.0-or-later), o único runtime AMPR incluído, sem modificações. Código-fonte:",
    logo: "O logotipo é de Xxcco:",
    unrar:
      "Arquivos .rar são lidos com a biblioteca UnRAR incluída, de Alexander Roshal (só extração, sob a licença do UnRAR).",
    language: "Idioma",
    system: "Sistema",
    notifications: "Avisar quando uma tarefa terminar",
    notificationsHint: "Aparece no console quando termina uma tarefa de criação, empacotamento ou transferência.",
    notificationsFailed: "Não foi possível alterar as notificações.",
  },
  // Edit.tsx
  edit: {
    details: "Detalhes",
    deleting: "Excluindo a origem",
    delete: "Excluir a origem",
    deleted: "Excluído:",
    filter: "Filtrar por caminho",
    filterLabel: "Filtrar arquivos por caminho",
    firstOf: (max, n) => `exibindo ${num(max)} de ${num(n)}, a partir do início`,
    fileCount: (n) => `${num(n)} ${plural(n, FILES)}`,
    noMatch: (f) => `Nenhum arquivo corresponde a “${f}”.`,
    noListing: "A lista de arquivos do arquivo compactado não é suportada.",
    noParam: "Sem sce_sys/param.json.",
    reader: "Leitor",
    leftovers: "Arquivos e pastas restantes",
    found: (n) => `${num(n)} ${plural(n, { one: "encontrado", many: "encontrados", other: "encontrados" })}`,
    leftoversNote: (app) =>
      `Saídas incompletas, geralmente de tarefas interrompidas. Remova só as que você reconhecer, e só quando nenhuma tarefa estiver em execução; o ${app} nunca as exclui sozinho.`,
    checked: (n) => `Pastas verificadas (${num(n)})`,
    facts: "Informações do jogo",
    explorer: "Arquivos do jogo",
    howTitle: "Como funciona",
    how: (app) => [
      "Abra a **pasta** ou a **imagem** de um jogo e altere-a como uma pasta num pendrive: **adicione e substitua arquivos** (arraste-os ou use “Adicionar”), **renomeie e exclua**.",
      `Nada muda **até você clicar em “Aplicar”**: suas alterações se acumulam aqui e então o ${app} as grava direto na imagem ou na pasta, de uma só vez, e verifica o resultado. **Não é uma unidade montada**: o ${app} registra suas alterações e as aplica ele mesmo.`,
      `Quando as alterações cabem, vão **direto para a imagem**; quando não cabem (sem espaço livre dentro dela, ou num formato que não as aceita), o ${app} cria ao lado uma nova imagem com suas alterações e a coloca no lugar depois de verificá-la. Em qualquer caso, se algo falhar, a imagem fica como estava.`,
      "Arquivos compactados podem ser navegados, mas não editados.",
    ],
    location: "Local no jogo",
    root: "Jogo",
    up: "Acima",
    upTip: "Subir uma pasta",
    name: "Nome",
    size: "Tamanho",
    actions: "Ações",
    items: (n) => `${num(n)} ${plural(n, { one: "item", many: "itens", other: "itens" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "pasta", many: "pastas", other: "pastas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "resultado", many: "resultados", other: "resultados" })}`,
    emptyFolder: "Esta pasta está vazia.",
    open: (name) => `Abrir ${name}`,
    archiveNote: "Arquivos compactados podem ser navegados, mas não editados. Para alterá-lo, crie antes uma pasta ou imagem a partir dele.",
    rebuildTimePkg: (t) => `Reconstruir comprime o jogo inteiro de novo: cerca de ${t} a ~26 MB/s em um PS5 (mais rápido em um computador). Edite uma cópia desempacotada para evitar isso.`,
    rebuildTimePs5: (t) => `Reconstruir grava a imagem inteira de novo (cerca de ${t} neste PS5).`,
    rebuildTimeHost: (t) => `Reconstruir grava a imagem inteira de novo (cerca de ${t} neste computador).`,
    rebuildTimeAny: (t) => `Reconstruir grava a imagem inteira de novo (cerca de ${t} em um PS5).`,
    nasNote: "Jogos em um NAS podem ser navegados e extraídos, mas não editados. Para alterá-lo, crie-o ou extraia-o para este dispositivo primeiro.",
    patchNote: "Este é um pacote de patch: contém só o que a atualização mudou. Os arquivos que ele contém por inteiro são listados; podem ser navegados e extraídos, mas não editados.",
    peerNote: "Jogos em outro Forge podem ser navegados e extraídos, mas não editados. Para alterá-lo, crie-o ou extraia-o para este dispositivo primeiro.",
    addFiles: "Adicionar arquivos",
    addFolder: "Adicionar pasta",
    pickAddFiles: "Escolha os arquivos a adicionar",
    pickAddFolder: "Escolha uma pasta a adicionar",
    peerRoot: "Não é possível adicionar um dispositivo inteiro: escolha uma pasta dentro dele.",
    renameItem: (name) => `Renomear ${name}`,
    removeItem: (name) => `Excluir ${name}`,
    renameTip: "Renomear (F2)",
    removeTip: "Excluir (tecla Delete)",
    removeTipMac: "Excluir (teclas fn+Apagar)",
    extractItem: (name: string) => `Extrair ${name}`,
    extractTip: "Copiar para uma pasta que você escolher",
    extractFolder: "Extrair pasta",
    extractFolderTip: "Copiar a pasta exibida para uma pasta que você escolher",
    extractGameTip: "Copiar o jogo inteiro para uma pasta que você escolher",
    extractNotOnDisk: "Esta pasta só existe nas suas alterações pendentes: aplique-as para extraí-la.",
    pickExtract: "Escolha onde extrair",
    extractPending: "Extrai os arquivos como estão agora, sem as suas alterações pendentes.",
    nameEmpty: "Digite um nome.",
    nameBad: "Um nome não pode conter / ou \\, nem ser . ou ..",
    nameTaken: (name) => `${name} já existe aqui.`,
    markNew: "Novo",
    markReplaced: "Substituído",
    markRenamed: "Renomeado",
    wasAt: (path) => `Antes em ${path}`,
    sizeLater: "Mais do que foi possível listar: o restante é lido quando as alterações forem aplicadas",
    dropInto: (folder) => `Solte para adicionar em ${folder}`,
    droppedGone: (path) => `${path}: o arquivo arrastado não está mais disponível`,
    pending: (n) =>
      plural(n, {
        one: `${num(n)} alteração ainda não aplicada`,
        many: `${num(n)} alterações ainda não aplicadas`,
        other: `${num(n)} alterações ainda não aplicadas`,
      }),
    undo: "Desfazer",
    undoTip: "Desfazer a última ação, com todas as suas alterações",
    discard: "Descartar tudo",
    discardTip: "Esquecer todas as alterações ainda não aplicadas",
    discardTitle: (n) =>
      plural(n, { one: `Descartar ${num(n)} alteração?`, many: `Descartar todas as ${num(n)} alterações?`, other: `Descartar todas as ${num(n)} alterações?` }),
    discardText: "Elas são esquecidas, e nada no jogo muda. Isso não pode ser desfeito.",
    discardNo: "Não",
    changesList: "Alterações, em ordem",
    willNotApply: (n) =>
      plural(n, {
        one: `${num(n)} alteração não pode ser aplicada como está. Desfaça-a ou descarte tudo.`,
        many: `${num(n)} alterações não podem ser aplicadas como estão. Desfaça-as ou descarte tudo.`,
        other: `${num(n)} alterações não podem ser aplicadas como estão. Desfaça-as ou descarte tudo.`,
      }),
    opAdd: (to) => <>Adicionar {to}</>,
    opAddFolder: (to) => <>Adicionar a pasta {to}</>,
    opDelete: (path) => <>Excluir {path}</>,
    opRename: (from, to) => <>Renomear {from} para {to}</>,
    opMkdir: (path) => <>Nova pasta {path}</>,
    opFrom: (src) => <>De {src}</>,
    whyInvalid: (path) => <>{path} não é um caminho válido no jogo.</>,
    whyMissing: (path) => <>{path} não está no jogo.</>,
    whyExists: (path) => <>{path} já existe.</>,
    whyFile: (path) => <>{path} é um arquivo.</>,
    whyFolder: (path) => <>{path} é uma pasta.</>,
    whyInto: (path) => <>{path} não pode ser movido para dentro de si mesmo.</>,
    conflictTitle: (n) =>
      plural(n, { one: `${num(n)} item já existe aqui`, many: `${num(n)} itens já existem aqui`, other: `${num(n)} itens já existem aqui` }),
    conflictIn: (folder) => <>Em {folder}:</>,
    conflictText:
      "“Substituir” coloca os novos no lugar deles: depois que as alterações forem aplicadas, o conteúdo antigo se perde de vez, e um arquivo com problema pode impedir o jogo de iniciar. Uma pasta adicionada sobre outra pasta é mesclada, e os arquivos com o mesmo nome são substituídos.",
    replace: "Substituir",
    skip: "Pular",
    cancel: "Cancelar",
    modeLabel: "Como aplicar",
    checking: "Verificando como as alterações podem ser aplicadas",
    nothingToChange: "Nada a mudar: o jogo fica como está.",
    checkFailed: "Não foi possível verificar as alterações:",
    checkAgain: "Verificar de novo",
    inPlace: "No local",
    rebuild: "Recriar",
    rebuildNeeded: "Precisa recriar",
    cantApply: "Não é possível aplicar",
    changedDirectly: "Alteração direta",
    folderText: "Gravadas direto na pasta.",
    inPlaceText: "Gravadas na imagem.",
    inPlaceNeeds: (need, free) => `Gravadas na imagem; precisam de ${need} dos ${free} livres dela.`,
    noRoom: (need, free) => `As alterações precisam de ${need}, e a imagem tem ${free} livres.`,
    alwaysRebuilt: "Imagens ffpfsc e FPKG são sempre recriadas.",
    rebuildText: "Uma nova imagem é gravada ao lado e a substitui depois de verificada.",
    rebuildNeeds: (need, free) =>
      `Uma nova imagem é gravada ao lado (são necessários cerca de ${need} livres; há ${free} livres) e a substitui depois de verificada.`,
    uploadLater: (n) =>
      plural(n, {
        one: `${num(n)} arquivo deste navegador é verificado depois de enviado, quando você clicar em “Aplicar”.`,
        many: `${num(n)} arquivos deste navegador são verificados depois de enviados, quando você clicar em “Aplicar”.`,
        other: `${num(n)} arquivos deste navegador são verificados depois de enviados, quando você clicar em “Aplicar”.`,
      }),
    decidedOnApply: (n) =>
      plural(n, {
        one: "Outras alterações dependem do arquivo deste navegador, então a forma de aplicá-las é decidida depois que ele for enviado, quando você clicar em “Aplicar”.",
        many: `Outras alterações dependem dos ${num(n)} arquivos deste navegador, então a forma de aplicá-las é decidida depois que eles forem enviados, quando você clicar em “Aplicar”.`,
        other: `Outras alterações dependem dos ${num(n)} arquivos deste navegador, então a forma de aplicá-las é decidida depois que eles forem enviados, quando você clicar em “Aplicar”.`,
      }),
    sourceLocked: "As alterações estão sendo aplicadas a este jogo. Escolha outra origem quando a tarefa terminar.",
    notOpened: (path) => <>{path} não foi aberto: as alterações estão sendo aplicadas a este jogo. Abra-o de novo quando a tarefa terminar.</>,
    apply: "Aplicar",
    applyTip: "Aplicar todas as alterações listadas aqui",
    applyTitle: (n) =>
      plural(n, { one: `Aplicar ${num(n)} alteração?`, many: `Aplicar ${num(n)} alterações?`, other: `Aplicar ${num(n)} alterações?` }),
    applyInPlace: (name) => <>Elas são gravadas direto em {name}. Depois de concluído, não há como desfazer.</>,
    applyRebuild: (name) => (
      <>Uma nova imagem com suas alterações é gravada ao lado de {name}, verificada e então colocada no lugar dela. Depois que ela substituir a imagem, não há como desfazer.</>
    ),
    closeImage: "Se o jogo estiver rodando a partir desta imagem, feche-o antes.",
    closeFolder: "Se o jogo estiver rodando a partir desta pasta, feche-o antes.",
    applyRestores: "Uma edição anterior desta imagem foi interrompida: primeiro a imagem volta a ser como era.",
    uploadingFile: (i, n) => `Enviando arquivo ${num(i)} de ${num(n)}`,
    uploading: (i, n, sent, total) => `Enviando arquivo ${num(i)} de ${num(n)}: ${sent} de ${total}`,
    uploadLabel: "Envio",
    uploadCancelled: "Envio cancelado. Nada no jogo mudou.",
    applyRunning: "Aplicando as alterações. A edição fica em espera até a tarefa terminar (veja “Fila”).",
    applyFailed: "As alterações não foram aplicadas:",
    applyRefused: "As alterações não podem ser aplicadas desta forma:",
    interrupted: "Uma edição anterior desta imagem foi interrompida. A imagem volta a ser como era antes que suas próximas alterações sejam verificadas ou aplicadas.",
  },
  // JobList.tsx
  jobs: {
    jobs: "Tarefas",
    pending: (n) => `${num(n)} em execução ou na fila`,
    allFinished: "todas concluídas",
    stages: {
      scan: "Lendo a origem",
      preflight: "Conferindo",
      check: "Conferindo",
      plan: "Planejando",
      compress: "Comprimindo",
      write: "Gravando",
      measure: "Medindo os pacotes",
      pack: "Empacotando",
      verify: "Verificando",
      finalize: "Finalizando",
      install: "Instalando",
    },
    steps: {
      scan: "Leitura",
      preflight: "Conferência",
      check: "Conferência",
      plan: "Planejamento",
      compress: "Compressão",
      write: "Gravação",
      measure: "Medição",
      pack: "Empacotamento",
      verify: "Verificação",
      finalize: "Finalização",
      install: "Instalação",
    },
    stepsLabel: "Etapas",
    stepDone: (s) => `Concluído: ${s}`,
    stepFailed: (s) => `Falhou: ${s}`,
    stepLater: (s) => `Ainda não iniciado: ${s}`,
    packingWriting: "Empacotando e gravando",
    rate: { read: "leitura", compress: "compressão", pack: "empacotamento", write: "gravação" },
    showPath: "Mostrar caminho",
    showFinder: "Mostrar no Finder",
    showExplorer: "Mostrar no Explorer",
    showFolder: "Mostrar na pasta",
    packTag: {
      lz4Pack: (level: string) => `LZ4: empacotar · ${level}`,
      lz4Unpack: "LZ4: desempacotar",
      zstdPack: (level: string) => `ZSTD: empacotar · ${level}`,
      level: (n: number) => `nível ${num(n)}`,
      zstdUnpack: "ZSTD: desempacotar",
      withRules: (tag) => `${tag} · regras próprias`,
    },
    edit: "Editar",
    editRebuild: "Editar · recriar",
    transfer: "Transferência",
    extract: "Extração",
    job: (id) => `Tarefa ${id}`,
    done: "Concluída",
    cancelled: "Cancelada",
    failed: "Falhou",
    cancelling: "Cancelando…",
    queued: "Na fila",
    dismiss: "Ocultar",
    retry: "Tentar de novo",
    retrying: "Iniciando",
    stopJob: "Parar tarefa?",
    cancel: "Cancelar",
    stopHint: "Pressione “Parar tarefa?” de novo em até 4 segundos para pará-la.",
    remoteLabel: (name, device) => `${name}, uma tarefa em ${device}`,
    remoteTitle: (device) => `Em execução em ${device} e usando arquivos deste dispositivo: aqui você só pode cancelá-la`,
    from: "de ",
    left: (d) => `tempo restante: ${d}`,
    inStep: (time) => `${time} nesta etapa`,
    outputPath: "Caminho da saída",
    sourceDeleted: (v) => `Origem excluída: ${v}`,
    sourceKept: (v) => `Origem mantida: ${v}`,
    sourcePartlyDeleted: (v) => `Origem excluída em parte: ${v}`,
    checks: "Verificações",
    verifiedPlain: (n) =>
      `Verificado: ${num(n)} ${plural(n, { one: "verificação aprovada", many: "verificações aprovadas", other: "verificações aprovadas" })}`,
    verifiedFull: (n) => `Verificação completa aprovada: ${num(n)} ${plural(n, CHECKS)}`,
    verifiedFast: (n) => `Verificação rápida aprovada: ${num(n)} ${plural(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `Dados verificados: ${checked} de ${total} em ${num(n)} ${plural(n, { one: "amostra", many: "amostras", other: "amostras" })}`,
    cantBuild: "Não é possível criar este jogo:",
    cantEdit: "Não é possível aplicar as alterações:",
    cantExtract: "Não é possível extrair:",
    cantTransfer: "Não é possível transferir:",
    cantBuildAs: (f) => (
      <>
        Não é possível criar este jogo como <b>{f}</b>:
      </>
    ),
    log: (n) => `Log (${num(n)})`,
    logLabel: "Log",
  },
  // Storage text shared by Files.tsx, Packages.tsx, Edit.tsx and common.tsx (FreeSpace)
  library: {
    cantDelete: "Não é possível excluir:",
    deleted: (path) => `Excluído: ${path}`,
    storageTip: "Espaço livre no armazenamento interno do console (/data)",
    free: (free, total) => (
      <>
        Espaço livre: <b>{free}</b> de {total}
      </>
    ),
    cards: "Exibição em cartões",
    list: "Exibição em lista",
    remove: (n) => `Excluir ${n}`,
    deleting: (n) => `Excluindo ${n}`,
    showAs: "Mostrar como",
  },
  // Storage.tsx, Files.tsx: the Storage tab (File explorer | Package installer)
  storage: {
    views: "Exibição do armazenamento",
    files: "Explorador de arquivos",
    title: "Explorador de arquivos",
    roots: "Unidades",
    location: "Local",
    folders: "Pastas",
    up: "Acima",
    upTip: "Subir uma pasta",
    refresh: "Atualizar",
    filter: "Filtrar por nome",
    filterLabel: "Filtrar esta pasta por nome",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "pasta", many: "pastas", other: "pastas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "resultado", many: "resultados", other: "resultados" })}`,
    firstOf: (max, n) => `exibindo ${num(max)} de ${num(n)}, a partir do início`,
    truncated: "Esta pasta tem mais de 10.000 itens: só os primeiros 10.000 são listados.",
    name: "Nome",
    size: "Tamanho",
    emptyFolder: "Esta pasta está vazia.",
    noMatch: (f) => `Nada aqui corresponde a “${f}”.`,
    noRoots: "Não há unidades disponíveis para explorar.",
    cantList: "Não é possível abrir esta pasta:",
    link: "Link",
    linkTip: "Um link: pode ser aberto, mas não renomeado, movido nem excluído aqui",
    mounted: "Montado",
    mountedTip: "Montado pelo ShadowMount+: feche o jogo e desative o ShadowMount+ para excluí-lo, renomeá-lo ou movê-lo",
    mountedNote: "As imagens marcadas como Montado estão em uso pelo ShadowMount+: feche o jogo e desative o ShadowMount+ para excluí-las, renomeá-las ou movê-las.",
    open: (n) => `Abrir ${n}`,
    more: (n) => `Mais ações para ${n}`,
    rename: (n) => `Renomear ${n}`,
    renameTip: "Renomear (F2)",
    move: (n) => `Mover ${n}`,
    moveTip: "Mover para outra pasta",
    moveTitle: (n) => `Mover ${n} para`,
    remove: (n) => `Excluir ${n}`,
    removeTip: "Excluir (tecla Delete)",
    removeTipMac: "Excluir (teclas fn+Apagar)",
    cantDelete: "Não é possível excluir:",
    cantRename: "Não é possível renomear:",
    cantMove: "Não é possível mover:",
    sameFolder: "Já está nessa pasta.",
    deleted: (path) => `Excluído: ${path}`,
    renamed: (path) => `Renomeado para ${path}`,
    moved: (path) => `Movido para ${path}`,
    devices: "Dispositivos",
    thisDevice: "Este dispositivo",
    otherDevice: "“Mover” fica em um só dispositivo: a tela “Transferência” copia entre dispositivos.",
    loadingDevices: "Carregando dispositivos",
    noDevice: "Nenhum dispositivo conectado",
    noDeviceHint: "Pareie um PS5 ou um computador, ou adicione um NAS, para gerenciar os arquivos dele aqui.",
    openConnect: "Abrir “Conectar”",
    dropMove: (folder) => `Solte para mover para ${folder}`,
    cantCopy: "Não é possível copiar:",
    copyNoFolder: "Abra primeiro uma pasta nesse dispositivo.",
    copyOne: (name: string, to: string) => `Copiando ${name} para ${to}. A transferência aparece em “Fila”.`,
    copyMany: (n: number, to: string) =>
      plural(n, {
        one: `Copiando ${num(n)} item para ${to}. A transferência aparece em “Fila”.`,
        many: `Copiando ${num(n)} de itens para ${to}. A transferência aparece em “Fila”.`,
        other: `Copiando ${num(n)} itens para ${to}. A transferência aparece em “Fila”.`,
      }),
  },
  // build/ (Build's pack settings and checks), JobList.tsx's pack tags
  pack: {
    help: "Como empacotar funciona",
    patch: "Um pacote de patch não pode ser empacotado.",
    additionalContent: "Conteúdo adicional (sem eboot.bin) não pode ser empacotado.",
    levels: { fast: "Rápido", medium: "Médio", max: "Máximo" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Recomendado. ZSTD 5: rápido, bom tamanho." },
        medium: { text: "ZSTD 9: menor, mais lento." },
        max: { text: "ZSTD 19: o menor; bem mais lento, mais memória." },
      },
      lz4: {
        fast: { tone: "good", text: "Recomendado. LZ4 padrão: o mais rápido." },
        medium: { text: "LZ4 HC 9: menor, mais lento." },
        max: { text: "LZ4 HC 12: o menor, o mais lento." },
      },
    },
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "O ampr_emu.index está desatualizado" : state === "damaged" ? "O ampr_emu.index está danificado" : "Não há ampr_emu.index"}: o empacotamento grava um novo.`,
    rules: {
      title: "Regras próprias (TOML)",
      titleWith: (name) => `Regras próprias (TOML): ${name}`,
      info: "Um perfil da estratégia de empacotamento no formato do ampr_pack.py do ampr_emu e do Lazy_AMPR decide quais arquivos são empacotados e quais ficam soltos. Vale a última regra que corresponde; os arquivos do sistema sempre ficam soltos.",
      warn: "Regras erradas podem deixar um jogo que não inicia. Guarde uma cópia.",
      pickTitle: "Estratégia de empacotamento (arquivo TOML)",
      clear: "Limpar",
    },
  },
  // PackHelp.tsx
  packHelp: {
    title: "Como empacotar funciona",
    close: "Fechar",
    body: () => (
      <>
        <h3>ZSTD</h3>
        <ol>
          <li>Empacota os assets do jogo em volumes ZSTD, numa pasta ou numa imagem.</li>
          <li><b>Rápido</b> (ZSTD 5) é ágil; <b>Médio</b> (9) e <b>Máximo</b> (19) são menores e mais lentos.</li>
          <li>Para um jogo empacotado com ZSTD, <b>Nenhum</b> o desempacota: os arquivos comuns voltam.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Alguns jogos podem não iniciar.</b> Guarde uma cópia.</li>
          <li>Empacota em volumes LZ4, numa pasta ou numa imagem: <b>Rápido</b> (LZ4), <b>Médio</b> (HC 9), <b>Máximo</b> (HC 12).</li>
          <li>Roda no ampr-emu-ng, como os pacotes ZSTD.</li>
          <li>Para um jogo empacotado com LZ4, <b>Nenhum</b> o desempacota.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>O único runtime AMPR que o Forge inclui. Lê pacotes ZSTD e LZ4; o empacotamento o instala em fakelib/.</li>
          <li>Ao criar, o Forge troca o ampr_emu de um jogo pelo ampr-emu-ng, a menos que as Configurações mantenham o ampr_emu. Um FPKG não leva runtime: o carregador do console o fornece.</li>
          <li>Desative a atualização de fakelib do ShadowMount+, senão ela substitui o runtime e o jogo não consegue ler seus pacotes.</li>
        </ol>

        <h3>Estratégia de empacotamento</h3>
        <ol>
          <li><b>Empacotar tudo exceto os arquivos do sistema</b> (ZSTD e LZ4): todos os arquivos exceto eboot.bin, executáveis e módulos, sce_sys, sce_module, fakelib e os arquivos .ini (dlc_emu.ini continua editável); o ampr-emu-ng serve o resto, áudio e vídeo incluídos.</li>
          <li>Ao empacotar, em <b>Regras próprias (TOML)</b>, escolha um perfil no formato do ampr_pack.py e do Lazy_AMPR, neste dispositivo ou em outro.</li>
          <li>Vale a última regra que corresponde; os arquivos do sistema sempre ficam soltos. Regras erradas podem deixar um jogo que não inicia.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "Dispositivos",
    thisDevice: "Este dispositivo",
    drives: "Unidades",
    location: "Local",
    parent: "Pasta acima",
    allDrives: "Todas as unidades",
    truncated: (n) => `Exibição limitada a ${num(n)} ${plural(n, { one: "entrada", many: "entradas", other: "entradas" })} do início da lista.`,
    loading: "Carregando",
    noDrives: "Nenhuma unidade encontrada.",
    noFolders: "Nenhuma pasta aqui.",
    noImages: "Nenhuma pasta, imagem de PS5 ou arquivo compactado aqui.",
    noFiles: (ext) => `Nenhuma pasta ou arquivo ${dotted(ext, " ou ")} aqui.`,
    chooseFiles: (n) => (n === 0 ? "Escolher arquivos" : plural(n, { one: `Escolher ${num(n)} arquivo`, many: `Escolher ${num(n)} arquivos`, other: `Escolher ${num(n)} arquivos` })),
    nothing: "Nenhuma pasta ou arquivo aqui.",
    cantList: "Não é possível abrir esta pasta:",
    cancel: "Cancelar",
    chooseFolder: "Escolher esta pasta",
    netOnlyHere: "Arquivos compactados e FPKG: só deste dispositivo.",
  },
  // bookmarks.tsx: the bookmarked folders (Transfer and the File explorer)
  bookmarks: {
    label: "Favoritos",
    add: "Adicionar esta pasta aos favoritos",
    remove: "Remover dos favoritos",
    removeNamed: (name: string) => `Remover ${name} dos favoritos`,
    full: "No máximo 9 favoritos",
  },
  // DeviceTabs.tsx
  deviceTabs: {
    add: "Adicionar um dispositivo",
    offline: "Offline",
    offlineNamed: (name: string) => `${name} · Offline`,
  },
  // common.tsx
  common: {
    folder: "Pasta",
    pickButton: "Escolher",
    pickAny: "Pasta, imagem ou arquivo compactado do jogo",
    source: "Origem",
    emptyTitle: "Escolha uma pasta, imagem ou arquivo compactado de jogo",
    accepted: "Origens aceitas",
    unknownTitle: "Título desconhecido",
    unsigned: (n: number) =>
      plural(n, {
        one: `${num(n)} executável não está assinado, então o jogo não vai iniciar. Ao converter o jogo, ele é assinado.`,
        many: `${num(n)} de executáveis não estão assinados, então o jogo não vai iniciar. Ao converter o jogo, eles são assinados.`,
        other: `${num(n)} executáveis não estão assinados, então o jogo não vai iniciar. Ao converter o jogo, eles são assinados.`,
      }),
    patch: "Patch",
    additionalContent: "Conteúdo adicional",
    reading: "Lendo a origem",
    unreadable: "Origem ilegível",
    backport: "Backport",
    emulators: "Emuladores",
    declares: (fw) => `declara ${fw}`,
    fwUnknown: "não foi possível determinar o firmware a partir dos executáveis (nenhum SDK legível)",
    emulatorList: (names) => `emuladores: ${names}`,
    otherEmulator: "Outro",
    sdk: (v) => `SDK ${v} (declarado)`,
    titleId: "ID do título",
    version: "Versão",
    firmware: "Firmware",
    fwDeclared: "declarado",
    fwBackport: "backport",
    fwExecutables: "executáveis",
    size: "Tamanho",
    files: (n) => `${num(n)} ${plural(n, FILES)}`,
    contentId: "ID do conteúdo",
    noAmpr: "eboot.bin não importa libSceAmpr",
    traceRuntime: "Runtime de rastreamento do LZ4 antigo",
    noRuntime: "Sem runtime",
    packed: (kind) => `Empacotado (${kind})`,
    zstdKept: "pacotes de assets ZSTD: a conversão os mantém",
    damaged: "Pacotes danificados",
    unpacked: "Não empacotado",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} em ${num(volumes)} ${plural(volumes, { one: "volume", many: "volumes", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)}% do tamanho`),
    index: {
      stale: "ampr_emu.index desatualizado: uma conversão o renova",
      damaged: "ampr_emu.index danificado: uma conversão o regrava",
      missing: "sem ampr_emu.index: uma conversão grava um",
    },
    runtimeAction: (a: "install" | "replace" | "keep" | "keepIfPacks") =>
      a === "install"
        ? "uma conversão instala o ampr-emu-ng incluído"
        : a === "replace"
          ? "uma conversão troca o runtime pelo ampr-emu-ng incluído"
          : a === "keep"
            ? "uma conversão o mantém (Configurações)"
            : "uma conversão o mantém se ele lê pacotes LZ4; senão, troca-o pelo ampr-emu-ng",
    backportLibs: (exclusive, n) =>
      `${exclusive ? "fakelib2/, montado sozinho" : "fakelib/"}, ${num(n)} ${plural(n, { one: "biblioteca", many: "bibliotecas", other: "bibliotecas" })} de backport`,
    findings: "Avisos da pré-verificação",
    dlcTitle: (n) => `DLCs embutidos (${num(n)})`,
    dlcLabel: "DLCs embutidos",
    dlcAll: (note) => `Todos os DLCs: ${note}.`,
    dlcIn: (folder, size) => `em ${folder}/, ${size}`,
    dlcNoData: "acesso liberado pelo emulador de DLC (dlc_emu.ini): só a licença, sem dados próprios",
    dlcStatus: (status) => `acesso liberado pelo emulador de DLC (dlc_emu.ini), status ${status || "não definido"}`,
    dlcMerged: "conteúdo mesclado às pastas do jogo",
    backportFiles: "Arquivos do backport",
    emulatorsIn: "Emuladores na fakelib",
    qr: (url) => `QR code: ${url}`,
    needsPassword: "Este arquivo compactado precisa de uma senha.",
    enterPassword: "Digitar senha",
    inArchive: (ext, size) => `em um arquivo ${ext} · ${size}`,
    archive: "Arquivo compactado",
    parts: (n) => `${num(n)} ${plural(n, { one: "parte", many: "partes", other: "partes" })}`,
    archiveRoot: (root) => `jogo em ${root}`,
    archiveAtRoot: "jogo na raiz do arquivo",
    solid: "sólido",
    encrypted: "protegido por senha",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "Excluir esta pasta?" : "Excluir esta imagem?"),
    text: (folder) =>
      folder
        ? "A pasta e tudo o que há nela serão excluídos para sempre: sem Lixeira, sem desfazer. Se a exclusão parar no meio, parte do conteúdo pode já ter sido apagada."
        : "O arquivo de imagem será excluído para sempre: sem Lixeira, sem desfazer.",
    no: "Não",
    yes: "Sim, excluir",
    archiveTitle: "Excluir este arquivo compactado?",
    archiveText:
      "O arquivo compactado será excluído para sempre: sem Lixeira, sem desfazer. As outras partes de um arquivo em várias partes ficam como estão.",
    fileTitle: "Excluir este arquivo?",
    fileText: "O arquivo será excluído para sempre: sem Lixeira, sem desfazer.",
  },
  // replace.tsx
  replace: {
    title: (n) =>
      plural(n, { one: `${num(n)} item já existe lá`, many: `${num(n)} itens já existem lá`, other: `${num(n)} itens já existem lá` }),
    inFolder: (folder) => <>Em {folder}:</>,
    more: (n) => `e mais ${num(n)}`,
    text: "“Substituir” coloca os novos no lugar deles: os arquivos antigos se perdem de vez, sem Lixeira e sem desfazer. Uma pasta sobre outra pasta com o mesmo nome é mesclada: os arquivos com o mesmo nome são substituídos, e todo o resto que está lá fica.",
    cancel: "Cancelar",
    replace: "Substituir",
  },
  // password.tsx
  password: {
    title: "Este arquivo compactado precisa de uma senha",
    text: "Digite a senha para abri-lo.",
    wrong: "A senha não está correta. Verifique-a e tente novamente.",
    label: "Senha",
    cancel: "Cancelar",
    unlock: "Desbloquear",
    checking: "Verificando",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "Não foi possível acessar o carregador de payloads na porta 9021. Execute seu autoloader (jailbreak) primeiro e depois abra o Dump Forge de novo.",
      missing:
        "Nenhuma cópia salva do Dump Forge foi encontrada (em /data/dumpforge/ ou em uma unidade USB). Carregue-o uma vez a partir de um PC; ele salva uma cópia para este ícone.",
      timeout:
        "O Dump Forge foi iniciado, mas não respondeu depois de 30 segundos. Ele ainda pode estar iniciando (um console saindo do modo de repouso é lento).",
      stopping: "O Dump Forge anterior ainda está parando (cancelando as tarefas e limpando). Aguarde um pouco e tente de novo.",
    },
    loadFailed: (e) => `Não foi possível carregar a página: ${e}`,
    noAnswer: "O Dump Forge não respondeu",
    cantStart: "Não foi possível iniciar o Dump Forge",
    retry: "Tentar de novo",
    starting: "Iniciando o Dump Forge…",
    client: {
      unreachable:
        "Não foi possível acessar o Dump Forge. Verifique se o payload ainda está em execução e se este dispositivo está na mesma rede.",
      noPlatform: "Não foi possível determinar se o Dump Forge está rodando em um PS5. Atualize a página para tentar de novo.",
      quitUnreachable: "Não foi possível acessar o Dump Forge para pará-lo.",
      stillAnswering: "O Dump Forge ainda está respondendo depois de 2 minutos; verifique o PS5.",
      gone: "o Dump Forge não informa mais esta tarefa (ela saiu do histórico de tarefas antes de o resultado ser visto)",
      uploadIncomplete: "O envio não foi concluído.",
    },
  },
  // target.tsx
  target: {
    kraken: {
      fast: { label: "Rápido", note: { tone: "good", text: "Recomendado. A criação mais rápida." } },
      balanced: {
        label: "Equilibrado",
        note: {
          text: "Cerca de 2,6% menor que “Rápido”, mas a compressão leva cerca de 6× mais tempo, com todos os núcleos ocupados o tempo todo.",
        },
      },
      smallest: {
        label: "Menor",
        note: {
          text: "Cerca de 2,7% menor que “Rápido”, mas a compressão leva cerca de 9× mais tempo, com todos os núcleos ocupados o tempo todo.",
        },
      },
      stored: {
        label: "Sem compressão",
        note: {
          text: "O pacote fica mais ou menos do tamanho do jogo. Tente se um pacote comprimido instala mas não inicia (relatado nos firmwares 8.60 e 10.40).",
        },
      },
    },
    imageInside: "Imagem interna",
    imageInsideLabel: "Imagem dentro do ffpfsc",
    compressionLevel: "Nível de compressão",
    compression: "Compressão",
    fullVerify: "Verificação completa",
    fullVerifyHint: "Relê cada byte para verificar a saída. Demora mais.",
    deleteSource: "Excluir a origem",
    deleteSourceHint: "Não dá para desfazer. A origem só é excluída depois de uma conversão bem-sucedida.",
    deleteSourceNas: "Um jogo em um NAS não pode ser excluído daqui.",
    separator: "Um nome não pode conter separador de pastas; use “Alterar” para a pasta.",
    badName: "Um nome não pode ficar em branco, começar com ponto, terminar com ponto ou espaço, nem ter caracteres de controle.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Longo demais: ${num(bytes)} ${plural(bytes, { one: "byte", many: "bytes", other: "bytes" })}, no máximo ${num(max)} (UTF-8).`
        : `Longo demais: ${num(bytes)} ${plural(bytes, { one: "byte", many: "bytes", other: "bytes" })} antes da extensão, no máximo ${num(max)} (UTF-8).`,
    pkgExt: (ext) => `Termina em .${ext}, mas FPKG é salvo como .pkg: a extensão escolhe o driver.`,
    otherExt: (ext, target) => `Termina em .${ext}, mas o destino é ${target}: a extensão escolhe o driver.`,
    extractInto: "Extrair nesta pasta",
    saveInto: "Salvar nesta pasta",
    output: "Saída",
    change: "Alterar",
    naming: "Gerando o nome…",
    chooseSourceFirst: "Escolha uma origem primeiro",
    fileName: "Nome do arquivo",
    savedAs: (name, dir) => `salvo como ${name} em ${dir}`,
    inDir: (dir) => `em ${dir}`,
    generate: "Gerar o nome pelo conteúdo",
  },
  // Formats.tsx (About's Output names) and target.tsx's name pattern: generated output names
  naming: {
    title: "Nomes de saída",
    parts: { title: "Título", title_id: "ID do título", version: "Versão", pack: "Tipo de pacote", region: "Região", fw: "Firmware" },
    template: "Modelo do nome",
    partsLabel: "Inserir uma parte",
    insert: (part: string) => `Inserir ${part}`,
    inUse: (part) => `${part} já está no modelo`,
    example: "Exemplo",
    length: "Tamanho",
    bytes: (n, max) => `${n} de ${max} bytes.`,
    room: (n) => plural(n, { one: `Resta ${num(n)} byte para o título.`, other: `Restam ${num(n)} bytes para o título.` }),
    noRoom: "Não sobra espaço para o título: ele é omitido.",
    versionOut: "A versão é omitida para abrir espaço.",
    cut: "O nome é cortado em 63 bytes.",
    idOnly: "O texto do modelo ocupa os 63 bytes, então o ID do título nomeia a saída.",
    errors: {
      long: (max: number) => `Um modelo tem no máximo ${max} bytes.`,
      open: "Uma { não tem a } de fechamento.",
      close: "Uma } não tem a { de abertura.",
      unknown: (token: string) => `{${token}} não é uma parte: use Título, ID do título, Versão, Tipo de pacote, Região ou Firmware.`,
      twice: (token) => `{${token}} aparece duas vezes no modelo: use cada parte uma só vez.`,
      char: (c: string) => `Um nome não pode conter ${c}.`,
      control: "Um nome não pode conter um caractere de controle.",
      needsName: "Um nome precisa de Título ou ID do título.",
    },
    notSaved: "Não salvo: os nomes continuam usando o último modelo válido.",
    reset: "Redefinir",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} está disponível`,
    update: "Atualizar",
    later: "Mais tarde",
    updating: "Atualizando…",
    updatingNote: "A nova versão está sendo baixada. Esta página vai recarregar sozinha com ela.",
    downloading: "O download está abrindo no navegador: substitua esta cópia pelo arquivo baixado (no Mac, arraste o app do .dmg para Aplicativos).",
    downloadingCli: "O download abre no navegador: substitua a CLI do ps5-dump-forge pelo arquivo baixado ou execute ps5-dump-forge update --install no computador em que ela roda.",
    failed: (v) => `Não foi possível atualizar para a v${v}`,
    timeout: (v) => `A v${v} não iniciou em um minuto. Se ela foi baixada, pare o Dump Forge e abra-o de novo para concluir a atualização.`,
    retry: "Tentar de novo",
    close: "Fechar",
  },
  // api.ts: prettyDuration's units
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "Este dispositivo",
    howItWorks: "Como conectar",
    ipAddress: "Endereço IP",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? plural(n, { one: `visto há ${num(n)} minuto`, many: `visto há ${num(n)} minutos`, other: `visto há ${num(n)} minutos` })
        : unit === "h"
          ? plural(n, { one: `visto há ${num(n)} hora`, many: `visto há ${num(n)} horas`, other: `visto há ${num(n)} horas` })
          : unit === "d"
            ? plural(n, { one: `visto há ${num(n)} dia`, many: `visto há ${num(n)} dias`, other: `visto há ${num(n)} dias` })
            : "visto agora mesmo",
    listenError: (e) => `Os outros dispositivos não conseguem acessar este: ${e}`,
    unavailable: (e) => `Conectar não está disponível: ${e}`,
    loading: "Carregando",
    discover: "Detecção automática",
    stop: "Parar",
    fileTransfer: "Transferência de arquivos",
    devices: "Dispositivos",
    show: "Mostrar",
    all: "Todos",
    onlineOf: (n, total) => `${num(n)} de ${num(total)} online`,
    sendFilesTo: (name) => `Enviar arquivos para ${name}`,
    userIs: (user) => `usuário ${user}`,
    nearby: "Por perto",
    nearbyNone: "Nenhum dispositivo novo encontrado",
    addDevice: "Adicionar um dispositivo",
    addressExample: "192.168.1.20 ou nas.local",
    addByProtocol: "Ou adicione por protocolo:",
    savedEmpty: "Nenhum dispositivo salvo ainda",
    savedEmptyHint:
      "A detecção automática encontra o Forge e armazenamento em rede nesta rede. Pareie um dispositivo e permita lá, ou salve um NAS.",
    online: "Online",
    offline: "Offline",
    computer: "Computador",
    address: "Endereço",
    keyId: "ID da chave",
    lastSeen: "Visto por último",
    status: "Status",
    never: "Nunca",
    unknown: "Desconhecido",
    forget: "Esquecer",
    forgetNote: "“Esquecer” remove o pareamento apenas neste dispositivo. Para usá-lo de novo, pareie-o novamente.",
    close: "Fechar",
    cancel: "Cancelar",
    pairTitle: "Parear um dispositivo",
    pair: "Parear",
    connectTo: "Conectar",
    probing: (address) => `Verificando ${address}`,
    pairing: "Pareando",
    waitingFor: (address) => `Aguardando ${address} permitir o pareamento`,
    checkNumber: "Número de verificação",
    checkHint: "Ele precisa ser igual ao número que o outro dispositivo mostra.",
    denied: "O outro dispositivo recusou o pareamento.",
    timedOut: "Ninguém permitiu o pareamento no outro dispositivo a tempo.",
    pairBusy: "O outro dispositivo tem outros pedidos de pareamento aguardando. Tente de novo em um minuto.",
    alreadyPaired: "Aquele dispositivo já tem este salvo. Esqueça-o lá primeiro e depois pareie de novo.",
    tooManyAttempts: "Pedidos recusados demais. Tente de novo em 10 minutos.",
    pairPromptTitle: (name) => `Parear com ${name}?`,
    pairPromptBody: (name, platform, ip) =>
      `${name} (${platform}, ${ip}) quer parear com este dispositivo. Um dispositivo pareado pode ler os arquivos deste dispositivo e adicionar novos.`,
    pairPromptCheck: (name) => `Só permita se ${name} mostrar o mesmo número de verificação.`,
    allow: "Permitir",
    deny: "Recusar",
    notPaired: "O outro dispositivo não reconhece mais este. Pareie-o novamente.",
    searching: "Procurando nesta rede",
    forge: "Forge",
    save: "Salvar",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `Adicionar um NAS ${protocol}`,
    nasEditTitle: (protocol) => `Editar NAS ${protocol}`,
    nasName: "Nome",
    nasAddressHint: "Endereço IP ou nome do host; adicione :porta quando não for 445",
    nasUser: "Nome de usuário",
    nasUserHint: "Deixe vazio para entrar como convidado",
    ftpAddressHint: "Endereço IP ou nome do host; adicione :porta quando não for 21",
    nfsAddressHint: "Endereço IP ou nome do host; adicione :porta quando não for 2049",
    ftpUserHint: "Deixe vazio para entrar anonimamente",
    ftpNote: "O FTP não é criptografado, nem o login.",
    nfsNote: "O NFS não tem login: o Forge se conecta como root (NFSv4.1, senão NFSv3), então a exportação do servidor precisa permitir este dispositivo. O NFS não é criptografado.",
    nasPassword: "Senha",
    nasPasswordKeep: "Deixe vazio para manter a senha salva",
    guest: "Convidado",
    passwordSaved: "Salva",
    passwordNone: "Nenhuma",
    edit: "Editar",
    nasForgetNote: "“Esquecer” remove este NAS e a senha salva dele deste dispositivo.",
    thisBrowser: "Este navegador",
    uploadPick: "Escolher arquivos para enviar",
    uploading: (name, i, n) => `Enviando ${name} (${num(i)} de ${num(n)})`,
    uploaded: "Envio concluído",
    uploadSkipped: (names) => `Não enviado, já há lá uma pasta ou um link com esse nome: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `${num(done)} de ${num(n)} arquivos foi enviado e fica neste dispositivo.`,
        other: `${num(done)} de ${num(n)} arquivos foram enviados e ficam neste dispositivo.`,
      }),
    cantUpload: "Não é possível enviar:",
    send: "Enviar",
    downloadHere: "Baixar deste dispositivo",
    wholeRoot: "Não é possível enviar um dispositivo, compartilhamento ou unidade inteiros: escolha uma pasta dentro dele.",
    helpPhone: "Escaneie o código QR para abrir esta página em um celular. Lá, “Transferência” envia os arquivos do celular para este dispositivo e baixa para o celular os arquivos deste dispositivo.",
    help: [
      "O Conectar liga o Dump Forge no seu PS5 e nos seus computadores da mesma rede.",
      "Para parear, inicie a “Detecção automática” e escolha “Parear” em “Por perto”, ou digite um endereço em “Adicionar um dispositivo”, e permita no outro dispositivo. Permita só se as duas telas mostrarem o mesmo número de verificação.",
      "Um NAS (SMB, FTP, NFS) é adicionado do mesmo jeito. Dispositivos pareados e NAS são origem de jogos e destino; os jogos deles não são editados lá.",
      "“Transferência”, no menu lateral, copia arquivos e pastas entre qualquer um deles. Um nome que já existe lá só é substituído depois que você confirmar.",
    ],
    helpSecurity: "Nada do que é enviado entre os dispositivos é criptografado: qualquer pessoa na sua rede pode ler. A senha salva de um NAS fica neste dispositivo sem criptografia. Use o Conectar só na sua rede de casa.",
    helpSpeed: "“Teste de velocidade”, na linha de um dispositivo pareado ou nos detalhes dele, envia 128 MiB para ele e busca 128 MiB de volta, sem gravar nada no disco, e mostra a velocidade da rede em cada sentido.",
    speedTest: "Teste de velocidade",
    speedTestWith: (name) => `Teste de velocidade com ${name}`,
    speedTesting: "Testando",
    speedResult: (up, down) => `Envio ${up} MB/s · Recebimento ${down} MB/s`,
    speedFailed: "O teste de velocidade falhou:",
    speedNote: "Envia 128 MiB e busca 128 MiB de volta; nada é gravado no disco.",
    installTitle: "Dump Forge no PS5",
    ps5Address: "Endereço do PS5",
    install: "Instalar no PS5",
    installHint: "Requer o carregador ELF do console acessível na rede (porta 9021).",
    findingRelease: "Procurando a versão no GitHub",
    installing: (v, host) => `Instalando o Dump Forge ${v} em ${host}`,
    installed: (v, host) => `O Dump Forge ${v} está iniciando em ${host}. Pareie-o em Adicionar um dispositivo quando estiver rodando.`,
    noRelease: "Não foi possível encontrar a versão do Dump Forge para PS5 no GitHub. Verifique a conexão com a internet deste computador e tente novamente.",
    otherPayload: "Qualquer outro payload ELF:",
    sendPayload: "Enviar um payload",
    pickPayload: "Escolha um payload ELF",
    sendingPayload: (name, host) => `Enviando ${name} para ${host}`,
    sentPayload: (name, size, host) => `${name} (${size}) enviado para ${host}.`,
    helpInstallTitle: "Instale o Dump Forge em um PS5 com um clique",
    helpInstallText: "Use o cartão de instalação do Dump Forge para enviar o payload ao console. Ele baixa do GitHub a versão correspondente (ou a mais recente) e a inicia.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "Instalador de pacotes",
    packages: "Pacotes",
    refresh: "Atualizar",
    scanning: "Procurando pacotes",
    none: "Nenhum pacote encontrado neste console.",
    noneHint: "Copie um arquivo .pkg para o armazenamento do console ou para um pendrive e pressione Atualizar.",
    truncated: "A lista pode estar incompleta",
    limits: "A busca parou em um dos seus limites: alguns pacotes podem estar faltando.",
    cantScan: "Não é possível buscar:",
    cantInstall: "Não é possível instalar:",
    content: { game: "Jogo", patch: "Patch", additional_content: "Conteúdo adicional", unknown: "Desconhecido" },
    install: "Instalar",
    installName: (name) => `Instalar ${name}`,
    started: (name: string) => `Instalando ${name}. A tarefa aparece em “Fila”.`,
    tag: "Instalação",
    title: "Instalar este pacote?",
    titleOn: (device) => `Instalar este pacote em ${device}?`,
    file: "Arquivo",
    game: "Jogo",
    titleId: "ID do título",
    version: "Versão",
    size: "Tamanho",
    text: "O PS5 busca o pacote no Dump Forge e o instala. Mantenha o Dump Forge aberto até a tarefa terminar.",
    reinstall: "Se este jogo já estiver instalado, o PS5 exclui a cópia instalada antes de instalar esta.",
    cancel: "Cancelar",
    confirm: "Instalar",
    installPkg: "Instalar pkg",
    installPkgOn: (device) => `Instalar um pacote em ${device}`,
    pickTitle: "Escolha um pacote para instalar",
    sent: (device: string) => `${device} está instalando o pacote. A tarefa aparece em “Fila”.`,
  },
  // Build's and Edit's Restore metadata online switch (build/options.tsx, Edit.tsx)
  metadata: {
    tag: "Metadados",
    cant: "Não foi possível restaurar os metadados:",
    label: "Restaurar metadados online",
    buildHint: "Antes da compilação, baixa do servidor de atualizações da Sony o param.json, as imagens, o som e as tabelas PlayGo desta versão e os coloca no resultado onde forem diferentes. O jogo em si não é alterado. Precisa de internet; sem ela a compilação segue como está.",
    buildSummary: "Coloca os metadados da Sony no resultado (o jogo não é alterado)",
    editHint: "Aplica a este jogo, com suas alterações, o param.json, as imagens, o som e as tabelas PlayGo desta versão do servidor de atualizações da Sony onde forem diferentes: uma pasta é alterada diretamente, um exfat ou ffpkg no lugar, e ffpfs, ffpfsc ou FPKG são reconstruídos. Precisa de internet; se o download falhar, nada é alterado.",
    editOnly: "Os metadados da Sony desta versão, aplicados a este jogo",
    editAlso: "Os metadados da Sony desta versão também são aplicados, baixados antes.",
    editConfirm: "Aplicar os metadados da Sony?",
    editConfirmText: "Os metadados são baixados primeiro; se isso falhar, nada é alterado.",
  },
  // App.tsx: the side rail
  rail: {
    build: "Criar",
    settings: "Configurações",
    queue: "Fila",
    transfer: "Transferência",
    collapse: "Recolher menu",
    expand: "Expandir menu",
  },
  // Queue.tsx
  queue: {
    empty: "Ainda não há tarefas. Criações, edições, transferências e instalações aparecem aqui.",
    counts: (running: number, queued: number, failed: number, done: number): string =>
      [
        running && `${num(running)} em execução`,
        queued && `${num(queued)} na fila`,
        failed && plural(failed, { one: `${num(failed)} com falha`, many: `${num(failed)} com falha`, other: `${num(failed)} com falha` }),
        done && plural(done, { one: `${num(done)} concluída`, many: `${num(done)} concluídas`, other: `${num(done)} concluídas` }),
      ]
        .filter(Boolean)
        .join(", ") || "Nada para mostrar",
    retryFailed: (n: number): string => (n > 0 ? `Repetir as que falharam (${num(n)})` : "Repetir as que falharam"),
    retrying: (i: number, n: number) => `Repetindo ${num(i)} de ${num(n)}`,
    clearFinished: "Limpar concluídas",
    filters: { all: "Todas", running: "Em execução", failed: "Com falha", done: "Concluídas" },
    filterLabel: "Mostrar",
    noneShown: "Nenhuma tarefa corresponde a este filtro.",
  },
  // Settings.tsx
  settings: {
    appearance: "Aparência",
    theme: "Tema",
    themeHint: "O escuro combina com uma TV; o claro, com um ambiente iluminado.",
    dark: "Escuro",
    light: "Claro",
    scheme: "Esquema de cores",
    schemeHint: "A cor de destaque em todo o app.",
    schemes: { abyss: "Abismo", ember: "Brasa", moss: "Musgo", cobalt: "Cobalto", dusk: "Crepúsculo", rose: "Rosa", sand: "Areia", volt: "Volt", mono: "Mono" },
    advanced: "Opções avançadas",
    advancedOpen: "Sempre mostrar as opções avançadas",
    advancedHint:
      "Mostra todos os pacotes e formatos, sem “Mais”, e mantém todas as seções abertas. Desative para manter Criar curto.",
    bundled: (version: string) => `${version} (incluído)`,
    replaceAmprEmu: "Substituir o ampr_emu pelo ampr-emu-ng",
    installedGames: "Mostrar jogos instalados",
    installedGamesHint:
      "Mostra os jogos instalados (/user/app) ao lado de /data nos seletores e no explorador de arquivos, para que o app.pkg de um FPKG instalado sirva de origem para Criar ou Editar. Somente leitura: nada ali é alterado ou excluído. Um jogo comercial não pode ser lido.",
    languageHint: "Os idiomas do sistema, até você escolher um.",
    notificationsGroup: "PS5",
  },
  // JobList.tsx: a finished job's result
  result: {
    file: "Saída",
    type: "Tipo",
    size: "Tamanho",
    files: "Arquivos",
    time: "Tempo",
    verified: "Verificado",
    verifiedFast: "Verificado (rápido)",
    verifiedFull: "Verificado (completo)",
    notVerified: "Nenhuma verificação informada",
    checks: "Verificações:",
    copy: "Copiar detalhes",
    copied: "Copiado",
    copyFailed: "Não é possível copiar aqui",
    ofSource: (pct: number) => `${num(pct, 1)}% dos arquivos do jogo`,
    filesN: (n: number) => plural(n, { one: `${num(n)} arquivo`, many: `${num(n)} arquivos`, other: `${num(n)} arquivos` }),
    to: "gravado em",
    took: (d: string) => `levou ${d}`,
    kinds: {
      pkg: "FPKG: instalável em um PS5 com suporte a FPKG",
      exfat: "Imagem exfat: o ShadowMount+ a monta, com gravação",
      ffpkg: "Imagem ffpkg (UFS2): o ShadowMount+ a monta, com gravação",
      ffpfs: "Imagem ffpfs (PFS): o ShadowMount+ a monta",
      ffpfsc: "Imagem ffpfsc (PFS comprimido): o ShadowMount+ a monta somente leitura",
      folder: "Pasta do jogo",
    } as Record<string, string | undefined>,
    zstdFolder: "Pasta empacotada com ZSTD",
    lz4Folder: "Pasta empacotada com LZ4",
    withPacks: (image: string, kind: string) => `${image}, arquivos em pacotes ${kind}`,
    edited: (what: string) => `${what}, alterado no lugar`,
    rebuilt: (what: string) => `${what}, reconstruído`,
    extracted: "Arquivos extraídos",
    transferred: "Arquivos copiados",
    installed: "Pacote entregue ao instalador do PS5",
  },
  // JobList.tsx: the log panel
  log: {
    lines: (n: number, of?: number): string =>
      of === undefined
        ? plural(n, { one: `${num(n)} linha`, many: `${num(n)} linhas`, other: `${num(n)} linhas` })
        : plural(of, { one: `${num(n)} de ${num(of)} linha`, many: `${num(n)} de ${num(of)} linhas`, other: `${num(n)} de ${num(of)} linhas` }),
    filter: "Filtrar",
    follow: "Rolagem automática",
    copy: "Copiar",
    copied: "Copiado",
    copyFailed: "Falha ao copiar",
    save: "Salvar",
    clear: "Limpar",
    cleared: "Limpo",
  },
  // common.tsx: the source card's fact chips
  sourceChips: {
    sdk: (v: string) => `SDK ${v}`,
    firmware: (v: string) => `FW ${v}+`,
  },
  // prefs.tsx: Advanced's badge
  advanced: {
    changed: "Alteradas",
    defaults: "Padrão",
  },
  // build/*: the Build screen
  build: {
    summary: "Resumo",
    addToQueue: "Adicionar à fila",
    queued: "Adicionado à fila",
    openQueue: "Abrir a fila",
    space: (free: string, dir: string, needed: string) => (
      <>
        {free} livres em <span className="path">{dir}</span>, são necessários cerca de {needed}
      </>
    ),
    needed: (needed: string) => `São necessários cerca de ${needed}`,
    spaceRaw: "O espaço é verificado como se nada fosse comprimido.",
    estimate: "estimativa",
    advanced: "Opções avançadas",
    options: "Opções",
    fastVerify: "Verificação rápida: cada estrutura é verificada e uma amostra dos dados é lida de volta.",
    fullVerify: "Verificação completa: cada byte é lido de volta.",
    deleted: "A origem é excluída depois de uma criação bem-sucedida. Não há como desfazer.",
    zstdBackport:
      "Os assets deste jogo estão empacotados com ZSTD e o índice ZSTD lista o backport: crie-o uma vez com Pacotes: Nenhum e depois remova o backport.",
    lz4Backport:
      "Os assets deste jogo estão empacotados com LZ4 e o manifesto LZ4 lista o backport: crie-o uma vez com Pacotes: Nenhum e depois remova o backport.",
    backportLeftOut: (n: number) =>
      plural(n, {
        one: `${num(n)} biblioteca de backport fica fora de fakelib/.`,
        many: `${num(n)} bibliotecas de backport ficam fora de fakelib/.`,
        other: `${num(n)} bibliotecas de backport ficam fora de fakelib/.`,
      }),
    packsTitle: "Destino",
    packs: "Pacotes",
    more: "Mais",
    moreTip: "Mais formatos",
    cards: {
      folder: "Arquivos comuns, copie para qualquer lugar, o ShadowMount+ o executa",
      pkg: "Instala no PS5 como um pacote de jogo",
      exfat: "Monta com o ShadowMount+ e abre em um PC",
      ffpkg: "Imagem UFS2 para o ShadowMount+, permite montagem com gravação",
      ffpfs: "Imagem PFS, sem compressão, só nomes de arquivo ASCII",
      ffpfsc: "A menor imagem: comprimida, sempre montada somente leitura",
      lz4: "Assets empacotados com LZ4, pasta comum",
    },
    zstdFolderCard: "Assets empacotados com ZSTD, pasta comum",
    packCards: {
      nonePlain: "Os arquivos do jogo como estão",
      unpack: "Grava os pacotes de volta como arquivos comuns",
      keepZstd: "Os pacotes ZSTD são mantidos byte a byte",
      keepLz4: "Os pacotes LZ4 são mantidos byte a byte",
      zstd: "Jogo menor, carregamento mais rápido, com o ampr-emu-ng",
      repackZstd: "Decodifica os pacotes LZ4 e empacota de novo com ZSTD",
      lz4: "Empacota os assets com LZ4, com o ampr-emu-ng",
      repackLz4: "Decodifica os pacotes ZSTD e empacota de novo com LZ4",
    },
    naming: "Saída: o nome está sendo gerado",
    output: (name: string, dir: string) => (
      <>
        Saída: <b className="path">{name}</b> em <span className="path">{dir}</span>
      </>
    ),
    choices: { none: "Nenhum", zstd: "ZSTD", lz4: "LZ4" },
    levelLabel: (level: string, engine: string, n: number) => `${level}, ${engine} ${n}`,
    levelTip: (engine: string, n: number, note: string) => `${engine} ${n}. ${note}`,
    sumKeepZstd: "Os pacotes ZSTD são mantidos byte a byte.",
    sumKeepLz4: "Os pacotes LZ4 são mantidos byte a byte.",
    sumKeepPlain: "Nenhum pacote é criado: os arquivos do jogo são convertidos como estão.",
    sumUnpack: (engine: string) => `Os pacotes ${engine} são gravados de volta como arquivos comuns.`,
    sumPack: (engine: string, level: string) => `Os arquivos do jogo são empacotados em volumes ${engine} (${level}).`,
    sumRepack: (from: string, to: string, level: string) => `Os pacotes ${from} são decodificados e empacotados de novo com ${to} (${level}).`,
    sumRules: (name: string) => `As regras personalizadas ${name} escolhem quais arquivos são empacotados.`,
    sumInside: (format: string, level: number) => `Dentro: ${format}, nível zlib ${level}.`,
    ngInstalled: "O ampr-emu-ng é instalado em fakelib/libSceAmpr.sprx.",
    ngReplaced: (old: string) => `O ${old} do jogo é substituído pelo ampr-emu-ng.`,
    ngReplacedTrace: "O antigo runtime de rastreamento do jogo é substituído pelo ampr-emu-ng.",
    ngKept: (old: string) => `O ${old} do jogo é mantido.`,
    ngUpdated: (v: string) => `O ampr-emu-ng${v} do jogo é substituído pelo incluído.`,
    ngSame: (v: string) => `O ampr-emu-ng${v} do jogo é mantido.`,
    pkgNoRuntime: "O runtime do AMPR fica fora do FPKG: o carregador do console o fornece.",
    recommended: "Recomendado",
    resultTitle: "Aproximadamente, após a criação",
    resultNow: "Jogo agora",
    resultCustomRules: "regras personalizadas: sem estimativa",
    backportTitle: "Backport",
    backportNone: "Este jogo não tem backport.",
    backportPlain: "Só numa conversão simples: sem empacotar nem desempacotar.",
  },
  // TransferPane.tsx: one device's pane of the Transfer screen
  transfer: {
    pane: (device) => `Arquivos em ${device}`,
    roots: "Unidades",
    location: "Local",
    up: "Acima",
    upTip: "Subir uma pasta",
    refresh: "Atualizar",
    filter: "Filtrar por nome",
    filterLabel: "Filtrar esta pasta por nome",
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "pasta", many: "pastas", other: "pastas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => plural(n, { one: `${num(n)} resultado`, many: `${num(n)} resultados`, other: `${num(n)} resultados` }),
    firstOf: (max, n) => `exibindo ${num(max)} de ${num(n)}, a partir do início`,
    truncated: "Esta pasta tem mais de 10.000 itens: só os primeiros 10.000 são listados.",
    name: "Nome",
    size: "Tamanho",
    select: (name) => `Selecionar ${name}`,
    open: (name) => `Abrir ${name}`,
    selected: (n) =>
      plural(n, { one: `${num(n)} item selecionado`, many: `${num(n)} itens selecionados`, other: `${num(n)} itens selecionados` }),
    clear: "Limpar",
    clearTip: "Limpar a seleção",
    dragItems: (n) => plural(n, { one: `${num(n)} item`, many: `${num(n)} itens`, other: `${num(n)} itens` }),
    dropHere: (path) => `Solte para enviar para ${path}`,
    notHere: "Abra uma pasta para enviar arquivos para cá.",
    emptyFolder: "Esta pasta está vazia.",
    noMatch: (f) => `Nada aqui corresponde a “${f}”.`,
    noRoots: "Não há unidades disponíveis para explorar.",
    cantList: "Não é possível abrir esta pasta:",
    loading: "Carregando",
    link: "Link",
    linkTip: "Um link: pode ser aberto, mas não selecionado",
    mounted: "Montado",
    mountedTip: "Montado pelo ShadowMount+: feche o jogo e desative o ShadowMount+ para selecioná-lo",
    withTitle: "Transferir com",
    loadingDevices: "Carregando dispositivos",
    noneTitle: "Nenhum dispositivo para transferir",
    noneHint: "Pareie outro Dump Forge ou adicione um NAS em “Conectar”, ou ligue um dispositivo salvo.",
    openConnect: "Abrir “Conectar”",
    deviceOffline: (device) => `${device} está offline. Os arquivos dele voltam a aparecer quando ele voltar.`,
    copyTo: (n, device) =>
      plural(n, {
        one: `Copiar ${num(n)} item para ${device}`,
        many: `Copiar ${num(n)} itens para ${device}`,
        other: `Copiar ${num(n)} itens para ${device}`,
      }),
    copyNoFolder: (device) => `Abra uma pasta em ${device} para copiar nela.`,
    installOn: (device) => `Instalar em ${device}`,
    dropFiles: "Solte arquivos aqui para enviá-los",
    uploadInto: (folder) => `Eles vão para ${folder}, a pasta exibida neste dispositivo.`,
    uploadNoFolder: "Abra uma pasta neste dispositivo para enviar arquivos para ela.",
    foldersLeftOut: (names) => `Pastas não são enviadas, só arquivos: ${names}.`,
    downloadFiles: (n) =>
      plural(n, { one: `Baixar ${num(n)} arquivo`, many: `Baixar ${num(n)} arquivos`, other: `Baixar ${num(n)} arquivos` }),
    downloadNone: "Selecione arquivos neste dispositivo para baixá-los (pastas não são baixadas).",
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default pt;
