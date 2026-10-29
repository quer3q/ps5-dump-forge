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
    convert: "Converter",
    active: (n) => `, ${num(n)} ${plural(n, { one: "ativa", many: "ativas", other: "ativas" })}`,
    edit: "Editar",
    pack: "Empacotar",
    storage: "Armazenamento",
    connect: "Conectar",
    disconnected: "Desconectado",
    connected: (n: number) =>
      plural(n, {
        one: `Conectado (${num(n)} dispositivo)`,
        many: `Conectado (${num(n)} dispositivos)`,
        other: `Conectado (${num(n)} dispositivos)`,
      }),
    about: "Sobre",
    prevTab: "Aba anterior (L2)",
    nextTab: "Próxima aba (R2)",
    offline: "Sem conexão, tentando de novo…",
    stop: "Parar o Dump Forge",
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
  // Convert.tsx
  convert: {
    target: "Destino",
    targetFormat: "Formato de destino",
    alreadyFolder: "a origem já é uma pasta",
    lz4Backport:
      "Os assets deste jogo estão empacotados com LZ4 e o manifesto LZ4 inclui o backport: desempacote o jogo na aba “Empacotar” primeiro e depois remova o backport aqui.",
    zstdBackport:
      "Os assets deste jogo estão empacotados com zstd e o índice zstd inclui o backport: desempacote o jogo na aba “Empacotar” primeiro e depois remova o backport aqui.",
    unreadable: "Não é possível criar: esta origem não pôde ser lida (veja “Origem”).",
    patchBlocked: "Não é possível criar: este é um pacote de patch, que contém só o que a atualização mudou. Abra-o em “Editar” para extrair os arquivos.",
    openInEdit: "Abrir em “Editar”",
    noContentId: "Não é possível criar como fpkg: o param.json deste jogo não tem Content ID, que um pacote exige.",
    blocked: (first, more) =>
      `Não é possível criar: ${first}${more > 0 ? ` (e mais ${num(more)}, veja “Origem”)` : ""}.`,
    amprZstdRuntime: "Esta conversão instala o ampr-emu-ng em fakelib/libSceAmpr.sprx, necessário para estes pacotes.",
    amprNgReplaced: (version) => `Substitui o ampr-emu-ng deste jogo pelo ampr_emu ${version}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Este jogo usa AMPR, mas não tem ampr_emu: a conversão instala o ampr_emu ${version} incluído.`;
      if (a === "trace") return `A conversão troca o ampr_emu deste jogo pelo ampr_emu ${version} incluído.`;
      if (a === "replace")
        return own
          ? `A conversão troca o ampr_emu ${own} deste jogo pelo ampr_emu ${version} incluído.`
          : `A conversão troca o ampr_emu deste jogo, que não informa a versão, pelo ampr_emu ${version} incluído.`;
      return own
        ? `O ampr_emu ${own} deste jogo é igual ou mais novo que o ${version} incluído: a conversão o mantém.`
        : `Este jogo já tem o ampr_emu ${version} incluído: a conversão o mantém.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "O ampr_emu.index não lista os arquivos deste jogo" : state === "damaged" ? "Não dá para ler o ampr_emu.index" : "Este jogo não tem ampr_emu.index"}: a conversão grava um novo.`,
    amprPkgIndex: "O pacote recebe um ampr_emu.index novo, gerado a partir dos arquivos dele.",
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
      zstd: { tagline: "Para jogos que usam AMPR, com ampr-emu-ng", cells: ["Carregamento até 80% mais rápido", "Até metade do tamanho do jogo"] },
      lz4: {
        tagline: "Experimental, para jogos que usam AMPR",
        cells: ["Carregamento 30–40% mais rápido, segundo os autores", "Cerca de metade do tamanho do jogo, segundo os autores"],
      },
      ffpkg: { tagline: "Recomendado para o ShadowMountPlus", cells: ["Velocidade total de imagem", FLAT] },
      exfat: { tagline: "Compatibilidade", cells: ["Velocidade total de imagem", FLAT] },
      ffpfs: { tagline: "Experimental", cells: ["Velocidade total de imagem", FLAT] },
      ffpfsc: {
        tagline: "O menor, experimental",
        cells: ["150–250 MB/s, pode engasgar", "Depende do nível de compressão, geralmente 30–60% menor"],
      },
      folder: { tagline: "Arquivos comuns do jogo", cells: ["Velocidade total da unidade", FLAT] },
    },
    features: [
      ["Converter", "entre pasta, ffpkg, exfat, ffpfs, ffpfsc e fpkg, nos dois sentidos."],
      ["Transferência", "enviar arquivos e converter jogos entre o seu PS5, computadores e um NAS via SMB, FTP ou NFS."],
      ["Editar", "os arquivos de um jogo como uma pasta e ver a capa, o firmware, o backport, os DLCs e os problemas de nomes de arquivo antes de criar."],
      ["Em streaming", "de qualquer origem para qualquer destino: sem cópia temporária, nem para a imagem dentro de um ffpfsc ou um jogo em um arquivo compactado."],
      ["Arquivos compactados", "um .zip, .7z ou .rar com um jogo é explorado, convertido ou empacotado como uma pasta."],
      ["USB e cartões SD", "gravado em pendrives e cartões formatados em exFAT ou FAT32."],
      ["Empacotar", "pacotes de assets zstd ou LZ4 (experimental) com reparo inteligente."],
      ["Armazenamento", "uma Biblioteca dos seus jogos e um Explorador de arquivos para renomear, mover e excluir."],
      ["Seguro", "seu jogo nunca é alterado a menos que você peça; arquivos inúteis são ignorados e nomes de arquivo inválidos são avisados antes."],
    ],
    about: "Sobre",
    compare: "Comparar formatos",
    compareLabel: "Comparação de formatos",
    format: "Formato",
    credits: "Créditos",
    upstream: (app) =>
      `Números dos autores: o que os autores do ampr_emu e do Lazy_AMPR relatam, sem medição do ${app}; variam de jogo para jogo. O ampr_emu 0.4.2.1 é uma versão de teste dos autores; problema conhecido: alguns jogos travam ao salvar.`,
    bundled: "Pastas empacotadas com LZ4 rodam no ampr_emu 0.4.2.1 de drakmor (GPL-3.0), incluído sem modificações. Código-fonte:",
    lazy: "O empacotamento LZ4 se baseia no Lazy_AMPR de Nazky:",
    ng: "Os pacotes zstd e LZ4 rodam no ampr-emu-ng. Código-fonte:",
    logo: "O logotipo é de Xxcco:",
    unrar:
      "Arquivos .rar são lidos com a biblioteca UnRAR incluída, de Alexander Roshal (só extração, sob a licença do UnRAR).",
    language: "Idioma",
    system: "Sistema",
    notifications: "Avisar quando uma tarefa terminar",
    notificationsHint: "Aparece no console para tarefas de conversão, empacotamento e transferência.",
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
    /** Edit's folds under the game card. */
    facts: "Informações do jogo",
    explorer: "Arquivos do jogo",
    howTitle: "Como funciona",
    /** The empty state's paragraphs. */
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
    archiveNote: "Arquivos compactados podem ser navegados, mas não editados. Para alterá-lo, converta-o antes em uma pasta ou imagem.",
    nasNote: "Jogos em um NAS podem ser navegados e extraídos, mas não editados. Para alterá-lo, converta-o ou extraia-o para este dispositivo primeiro.",
    patchNote: "Este é um pacote de patch: contém só o que a atualização mudou. Os arquivos que ele contém por inteiro são listados; podem ser navegados e extraídos, mas não editados.",
    peerNote: "Jogos em outro Forge podem ser navegados e extraídos, mas não editados. Para alterá-lo, converta-o ou extraia-o para este dispositivo primeiro.",
    /** The explorer's changes: none is made until Apply. */
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
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
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
    /** How the changes go in (the pending bar), from core's check before Apply. */
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
    alwaysRebuilt: "Imagens ffpfsc e fpkg são sempre recriadas.",
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
    applyRunning: "Aplicando as alterações. A edição fica em espera até a tarefa abaixo terminar.",
    applyFailed: "As alterações não foram aplicadas:",
    applyRefused: "As alterações não podem ser aplicadas desta forma:",
    applyCancelled: "A tarefa foi cancelada. Suas alterações continuam listadas aqui.",
    folderPartly: "Parte delas pode já estar na pasta: os arquivos mostrados são o que ela contém agora.",
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
      zstdPack: (level: string) => `zstd: empacotar · ${level}`,
      level: (n: number) => `nível ${num(n)}`,
      zstdUnpack: "zstd: desempacotar",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
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
    result: (bytes, n) => `${bytes}, ${num(n)} ${plural(n, FILES)} →`,
    outputPath: "Caminho da saída",
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
  // Library.tsx
  library: {
    pickTitle: "Escolha uma pasta para a Biblioteca",
    scanningIn: (name) => `Escaneando ${name}`,
    scanning: "Escaneando",
    cantScanFolder: "Não é possível escanear esta pasta",
    chooseAnother: "Escolher outra pasta",
    pickPrompt: "Escolha uma pasta com a sua biblioteca para escanear",
    choose: "Escolher pasta",
    cantScan: "Não é possível escanear a Biblioteca:",
    cantDelete: "Não é possível excluir:",
    deleted: (path) => `Excluído: ${path}`,
    truncated: "Limite de escaneamento atingido. Esta lista pode estar incompleta.",
    limits: "A busca para em seis níveis de pastas e depois de cerca de 15 segundos de escaneamento.",
    noRoots: "Não há pastas disponíveis para escanear.",
    none: "Nenhum jogo encontrado. A busca vai até seis níveis de profundidade e ignora pastas do sistema. Outras origens ainda podem ser escolhidas em “Editar” ou “Converter”.",
    skipped: "Locais e arquivos ignorados",
    library: "Biblioteca",
    storageTip: "Espaço livre no armazenamento interno do console (/data)",
    free: (free, total) => (
      <>
        Espaço livre: <b>{free}</b> de {total}
      </>
    ),
    refresh: "Atualizar",
    cards: "Exibição em cartões",
    list: "Exibição em lista",
    games: "Jogos",
    edit: (n) => `Editar ${n}`,
    convert: (n) => `Converter ${n}`,
    remove: (n) => `Excluir ${n}`,
    deleting: (n) => `Excluindo ${n}`,
    lz4Tip: "Pacotes de assets LZ4",
    zstdTip: "Pacotes de assets zstd",
    showAs: "Mostrar como",
    kinds: "Mostrar tipo",
    all: "Todos",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n, path) => (
      <>
        <b>{num(n)}</b> {plural(n, { one: "jogo em", many: "jogos em", other: "jogos em" })} {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "Exibição do armazenamento",
    library: "Biblioteca",
    files: "Explorador de arquivos",
    title: "Explorador de arquivos",
    pickTitle: "Escolha uma pasta para explorar",
    pickPrompt: "Escolha uma pasta para explorar os arquivos dela",
    choose: "Escolher pasta",
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
    otherDevice: "“Mover” fica em um só dispositivo: arraste para a aba de outro dispositivo para copiar para lá.",
    dropCopy: (folder) => `Solte para copiar para ${folder}`,
    dropMove: (folder) => `Solte para mover para ${folder}`,
    cantCopy: "Não é possível copiar:",
    copyNoFolder: "Abra primeiro uma pasta nesse dispositivo.",
    copyOne: (name, to) => `Copiando ${name} para ${to}. A transferência aparece em “Conectar”.`,
    copyMany: (n, to) =>
      plural(n, {
        one: `Copiando ${num(n)} item para ${to}. A transferência aparece em “Conectar”.`,
        many: `Copiando ${num(n)} de itens para ${to}. A transferência aparece em “Conectar”.`,
        other: `Copiando ${num(n)} itens para ${to}. A transferência aparece em “Conectar”.`,
      }),
    openConnect: "Abrir “Conectar”",
  },
  // Pack.tsx
  pack: {
    title: "Empacotar",
    help: "Como empacotar funciona",
    engine: "Tipo de pacotes",
    engines: { zstd: "zstd", lz4: "LZ4" },
    actions: { pack: "Empacotar", unpack: "Desempacotar" },
    info: (e, packed) =>
      e === "zstd"
        ? packed
          ? "Grava os pacotes zstd de volta como arquivos comuns."
          : "Empacota os assets do jogo em volumes zstd."
        : packed
          ? "Experimental. Grava os pacotes LZ4 de volta como arquivos comuns."
          : "Experimental: alguns jogos podem não iniciar. Empacota os assets do jogo em volumes LZ4.",
    chooseGame: "Escolha um jogo primeiro.",
    noAmprGame: (e) => `Este jogo não usa AMPR, então os pacotes ${e === "zstd" ? "zstd" : "LZ4"} não podem ser usados.`,
    patch: "Um pacote de patch não pode ser empacotado.",
    additionalContent: "Conteúdo adicional (sem eboot.bin) não pode ser empacotado.",
    unreadable: "Não é possível iniciar: a origem não pôde ser lida (veja “Origem”).",
    blocked: (a, first, more) =>
      `Não é possível ${a === "pack" ? "empacotar" : "desempacotar"}: ${first}${more > 0 ? ` (e mais ${num(more)}, veja “Origem”)` : ""}.`,
    target: "Destino",
    packFolder: (e) => `Recomendado. O mais rápido, menor latência: uma nova pasta do jogo, com os assets em pacotes ${e === "zstd" ? "zstd" : "LZ4"}.`,
    levels: { fast: "Rápido", medium: "Médio", max: "Máximo" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Recomendado. zstd 5: rápido, bom tamanho." },
        medium: { text: "zstd 9: menor, mais lento." },
        max: { text: "zstd 19: o menor; bem mais lento, mais memória." },
      },
      lz4: {
        fast: { tone: "good", text: "Recomendado. LZ4 padrão: o mais rápido." },
        medium: { text: "LZ4 HC 9: menor, mais lento." },
        max: { text: "LZ4 HC 12: o menor, o mais lento." },
      },
    },
    runtime: "Runtime",
    runtimeNotes: {
      ng: { tone: "good", text: "Recomendado. Lê pacotes zstd e LZ4." },
      ampr_emu: { text: "O ampr_emu original." },
    },
    ngNote: "Precisa do ampr-emu-ng: Empacotar o instala em fakelib/. Desative a atualização de fakelib do ShadowMount+, senão ela substitui o runtime.",
    installsRuntime: (version) => `Não há ampr_emu aqui: Empacotar instala o ampr_emu ${version}.`,
    freshIndex: (state) =>
      `${state === "stale" ? "O ampr_emu.index está desatualizado" : state === "damaged" ? "O ampr_emu.index está danificado" : "Não há ampr_emu.index"}: Empacotar grava um novo.`,
    direct: "Empacota direto na imagem; lê o jogo duas vezes.",
    unpackFolder: "Uma nova pasta do jogo com arquivos comuns.",
    unpackHint: (n) =>
      n === null
        ? "Grava os arquivos empacotados de volta como arquivos comuns."
        : `Grava de volta ${num(n)} ${plural(n, { one: "arquivo empacotado como arquivo comum", many: "arquivos empacotados como arquivos comuns", other: "arquivos empacotados como arquivos comuns" })}.`,
    likelyFail: (fmt) => <>
      Provavelmente vai falhar como <b>{fmt}</b>: esse formato recusa os nomes de arquivo listados em “Origem”.
    </>,
    collides: (a, folder) =>
      `Uma tarefa já está gravando ${folder ? "esta pasta" : "este arquivo"}: ${a === "pack" ? "“Empacotar”" : "“Desempacotar”"} precisa de outro nome.`,
    already: (a, fmt) => <>
      Este jogo já está sendo gravado como <b>{fmt}</b>; clicar em {a === "pack" ? "“Empacotar”" : "“Desempacotar”"} de novo gera uma segunda cópia.
    </>,
    emptyTitle: "Escolha um jogo que use AMPR",
    experimental: "Experimental: guarde uma cópia do jogo original.",
    source: "Origem",
  },
  // PackHelp.tsx
  packHelp: {
    title: "Como empacotar funciona",
    close: "Fechar",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>Empacota os assets do jogo em volumes zstd, numa pasta ou numa imagem. Só para jogos que usam AMPR.</li>
          <li><b>Rápido</b> (zstd 5) é ágil; <b>Médio</b> (9) e <b>Máximo</b> (19) são menores e mais lentos.</li>
          <li>Um jogo empacotado mostra <b>Desempacotar</b>: os arquivos comuns voltam.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Experimental</b>: alguns jogos podem não iniciar. Guarde uma cópia.</li>
          <li>Empacota em volumes LZ4, numa pasta ou numa imagem: <b>Rápido</b> (LZ4), <b>Médio</b> (HC 9), <b>Máximo</b> (HC 12).</li>
          <li>Runtime: <b>ampr-emu-ng</b> (padrão) ou o <b>ampr_emu</b> original.</li>
          <li>Um jogo empacotado mostra <b>Desempacotar</b>.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>Lê pacotes zstd e LZ4; Empacotar o instala em fakelib/.</li>
          <li>Desative a atualização de fakelib do ShadowMount+, senão ela substitui o runtime e o jogo não consegue ler seus pacotes.</li>
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
    runtime: {
      forge_release: "de release do Forge",
      forge_trace: "runtime de rastreamento do LZ4 antigo",
      zstd: "ampr-emu-ng",
      other: "outro",
      none: "nenhum",
    },
    packed: "Empacotado",
    zstdKept: "pacotes de assets zstd: a conversão os mantém",
    damaged: "Pacotes danificados",
    plain: "Não empacotado",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} em ${num(volumes)} ${plural(volumes, { one: "volume", many: "volumes", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)}% do tamanho`),
    runtimeIs: (label) => `runtime ${label}`,
    index: {
      stale: "ampr_emu.index desatualizado: uma conversão o renova",
      damaged: "ampr_emu.index danificado: uma conversão o regrava",
      missing: "sem ampr_emu.index: uma conversão grava um",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `uma conversão instala o ampr_emu ${version} incluído`
        : a === "replace"
          ? `uma conversão troca o runtime pelo ampr_emu ${version} incluído`
          : `ampr_emu ${version}: uma conversão o mantém`,
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
    wrong: "A senha não está correta.",
    label: "Senha",
    cancel: "Cancelar",
    unlock: "Desbloquear",
    checking: "Verificando…",
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
    format: {
      folder: { text: "Desempacota uma imagem ou um pacote de volta em uma pasta comum de jogo." },
      exfat: {
        text: "Para jogos que dão problema como ffpkg e só rodam como conteúdo de unidade externa. Também abre em um Mac ou PC.",
      },
      ffpkg: {
        text: "O sistema de arquivos próprio do PS5 (UFS2). Permite montagem com gravação.",
      },
      ffpfs: {
        text: "O sistema de arquivos PFS do PS5, sem compressão. Os nomes de arquivo precisam ser ASCII simples.",
      },
      ffpfsc: {
        text: "O menor: um contêiner comprimido em torno de uma imagem, sempre montado como somente leitura.",
      },
      pkg: { tone: "good", text: "Recomendado. Roda como um jogo nativo, mas só se o seu firmware permitir." },
      lz4: {
        tone: "warn",
        text: "Experimental. Uma pasta de jogo com os assets empacotados em volumes LZ4, que o emulador AMPR incluído (ampr_emu) desempacota enquanto o jogo roda. Só para jogos que usam AMPR.",
      },
    },
    innerFfpkg: { text: "O sistema de arquivos próprio do PS5 (UFS2)." },
    level0: { text: "Sem compressão: cada bloco é armazenado como está. O arquivo fica do tamanho da imagem." },
    levelDefault: {
      tone: "good",
      text: "Recomendado: é o padrão do zlib. O nível 1 cria cerca de 3× mais rápido, com um arquivo cerca de 3% maior; o nível 9 gera um arquivo só um pouco menor.",
    },
    levelLow: {
      text: "Cria mais rápido que o 6 (nível 1: cerca de 3×); o arquivo sai maior (nível 1: cerca de 3%).",
    },
    levelHigh: {
      text: "Quase do mesmo tamanho que o 6 (nível 9: 0,04% menor), mas a compressão demora mais (nível 9: cerca de 25%), com todos os núcleos ocupados o tempo todo.",
    },
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
    separator: "Um nome não pode conter separador de pastas; use “Alterar” para a pasta.",
    badName: "Um nome não pode ficar em branco, começar com ponto, terminar com ponto ou espaço, nem ter caracteres de controle.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `Longo demais: ${num(bytes)} ${plural(bytes, { one: "byte", many: "bytes", other: "bytes" })}, no máximo ${num(max)} (UTF-8).`
        : `Longo demais: ${num(bytes)} ${plural(bytes, { one: "byte", many: "bytes", other: "bytes" })} antes da extensão, no máximo ${num(max)} (UTF-8).`,
    pkgExt: (ext) => `Termina em .${ext}, mas fpkg é salvo como .pkg: a extensão escolhe o driver.`,
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
    generatePattern: "[nome do jogo]-[ID do título]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} está disponível`,
    update: "Atualizar",
    later: "Mais tarde",
    updating: "Atualizando…",
    updatingNote: "A nova versão está sendo baixada. Esta página vai recarregar sozinha com ela.",
    downloading: "Baixando no navegador: descompacte o arquivo compactado e substitua esta cópia pelo conteúdo dele.",
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
    details: (name) => `Detalhes de ${name}`,
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
    pickSendFiles: "Arquivos a enviar",
    pickSendFolder: "Pasta a enviar",
    pickSaveTo: "Onde salvar",
    thisBrowser: "Este navegador",
    browserHint: "Arquivos deste celular ou computador vão para uma pasta neste dispositivo, e um arquivo deste dispositivo é salvo por este navegador.",
    uploadTo: "Pasta de destino do envio",
    uploadPick: "Escolher arquivos para enviar",
    uploading: (name, i, n) => `Enviando ${name} (${num(i)} de ${num(n)})`,
    uploaded: "Envio concluído",
    uploadSkipped: (names) => `Não enviado, já há lá uma pasta ou um link com esse nome: ${names}.`,
    uploadedBefore: (done, n) =>
      plural(done, {
        one: `${num(done)} de ${num(n)} arquivos foi enviado e fica neste dispositivo.`,
        other: `${num(done)} de ${num(n)} arquivos foram enviados e ficam neste dispositivo.`,
      }),
    cantStart: "Não é possível iniciar a transferência:",
    cantUpload: "Não é possível enviar:",
    pickDownload: "Arquivo a baixar",
    transferIntro: "Envie arquivos ou uma pasta de qualquer dispositivo para uma pasta aqui, em um dispositivo pareado ou em um NAS. A transferência roda como uma tarefa na lista de tarefas.",
    send: "Enviar",
    sendWhat: "O que enviar",
    sendTo: "Enviar para",
    sendNeedsDevice: "Conecte outro dispositivo primeiro: pareie um Forge ou adicione um NAS.",
    nothingChosen: "Nada escolhido ainda",
    chooseFiles: "Escolher arquivos",
    chooseFolder: "Escolher uma pasta",
    startTransfer: "Iniciar",
    uploadHere: "Enviar para este dispositivo",
    downloadHere: "Baixar deste dispositivo",
    consoleNoFiles: "O navegador do PS5 não consegue escolher arquivos: abra esta página em um celular ou computador.",
    wholeRoot: "Não é possível enviar um dispositivo, compartilhamento ou unidade inteiros: escolha uma pasta dentro dele.",
    helpPhone: "Escaneie o código QR para abrir esta página em um celular. Lá, “Enviar para este dispositivo” e “Baixar deste dispositivo” em “Transferência de arquivos” movem os arquivos do celular.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "O Conectar liga o Dump Forge no seu PS5 e nos seus computadores da mesma rede.",
      "Para parear, inicie a “Detecção automática” e escolha “Parear” em “Por perto”, ou digite um endereço em “Adicionar um dispositivo”, e permita no outro dispositivo. Permita só se as duas telas mostrarem o mesmo número de verificação.",
      "Um NAS (SMB, FTP, NFS) é adicionado do mesmo jeito. Dispositivos pareados e NAS são origem de jogos e destino; os jogos deles não são editados lá.",
      "“Transferência de arquivos” copia arquivos e pastas entre qualquer um deles. Um nome que já existe lá só é substituído depois que você confirmar.",
    ],
    helpSecurity: "Nada do que é enviado entre os dispositivos é criptografado: qualquer pessoa na sua rede pode ler. A senha salva de um NAS fica neste dispositivo sem criptografia. Use o Conectar só na sua rede de casa.",
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
    started: (name) => `Instalando ${name}. A tarefa aparece abaixo.`,
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
    sent: (device) => `${device} está instalando o pacote. A tarefa aparece abaixo.`,
  },
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default pt;
