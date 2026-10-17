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
    address: (url) => `Endereço: ${url}. Mostrar como conectar, com um QR code`,
    screens: "Telas",
    convert: "Converter",
    active: (n) => `, ${num(n)} ${plural(n, { one: "ativa", many: "ativas", other: "ativas" })}`,
    edit: "Editar",
    library: "Biblioteca",
    about: "Sobre",
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
    couldntStop: "Não foi possível parar o Dump Forge",
    close: "Fechar",
    tryAgain: "Tentar de novo",
    stopped: "O Dump Forge foi parado",
    startAgain: (ps5) => (ps5 ? "Para continuar, abra de novo o bloco do Dump Forge no PS5." : "Para continuar, execute de novo o mesmo comando ps5-dump-forge serve."),
    confirmStop: (web: boolean) => (web ? "Parar o Dump Forge?" : "Sair do PS5 Dump Forge?"),
    stopNote: (web, ps5) => (!web ? "Nenhuma tarefa em andamento." : ps5 ? "Para iniciá-lo de novo, abra o bloco do Dump Forge no PS5." : "Para iniciá-lo de novo, execute outra vez o mesmo comando ps5-dump-forge serve."),
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
      "Os assets deste jogo estão empacotados com LZ4 e o manifesto LZ4 inclui o backport: desempacote o jogo na aba “LZ4” primeiro e depois remova o backport aqui.",
    unreadable: "Não é possível criar: esta origem não pôde ser lida (veja “Origem”).",
    noContentId: "Não é possível criar como fpkg: o param.json deste jogo não tem Content ID, que um pacote exige.",
    blocked: (first, more) =>
      `Não é possível criar: ${first}${more > 0 ? ` (e mais ${num(more)}, veja “Origem”)` : ""}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `Este jogo usa AMPR, mas não tem ampr_emu: a conversão instala o ampr_emu ${version} incluído.`;
      if (a === "trace") return `A conversão troca o runtime de rastreamento do LZ4 pelo ampr_emu ${version} de release incluído.`;
      if (a === "replace")
        return own
          ? `A conversão troca o ampr_emu ${own} deste jogo pelo ampr_emu ${version} incluído.`
          : `A conversão troca o ampr_emu deste jogo, que não informa a versão, pelo ampr_emu ${version} incluído.`;
      return own
        ? `O ampr_emu ${own} deste jogo é igual ou mais novo que o ${version} incluído: a conversão o mantém.`
        : `Este jogo já tem o ampr_emu ${version} incluído: a conversão o mantém.`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "O ampr_emu.index não lista os arquivos deste jogo" : state === "damaged" ? "Não dá para ler o ampr_emu.index" : "Este jogo não tem ampr_emu.index"}: a conversão grava um novo${traces ? " e deixa de fora os rastreamentos antigos" : ""}.`,
    amprPkgIndex: (traces) =>
      `O pacote recebe um ampr_emu.index novo, gerado a partir dos arquivos dele${traces ? ", sem os rastreamentos antigos" : ""}.`,
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
      ["Editar", "os arquivos de um jogo como uma pasta e ver a capa, o firmware, o backport, os DLCs e os problemas de nomes de arquivo antes de criar."],
      ["Excluir", "um jogo do disco, depois da sua confirmação."],
      ["LZ4", "rastrear um jogo AMPR e depois empacotar os assets dele com reparo inteligente (experimental)."],
      ["Em streaming", "de qualquer origem para qualquer destino: sem cópia temporária, nem para a imagem dentro de um ffpfsc ou um jogo em um arquivo compactado."],
      ["Arquivos compactados", "um .zip, .7z ou .rar com um jogo é explorado, convertido ou empacotado como uma pasta."],
      ["Biblioteca", "os jogos deste computador ou console, prontos para converter e editar."],
      ["Seguro", "seu jogo nunca é alterado a menos que você peça; arquivos inúteis são ignorados e nomes de arquivo inválidos são avisados antes."],
      ["USB e cartões SD", "gravado em pendrives e cartões formatados em exFAT ou FAT32."],
      ["No lugar ou refeito", "as edições vão direto na imagem quando cabem; senão, vão para uma nova imagem que a substitui depois de verificada."],
    ],
    howToConnect: "Como conectar",
    connectText: (url) => (
      <>Abra {url} em um navegador de qualquer dispositivo da sua rede local ou escaneie este código.</>
    ),
    close: "Fechar",
    about: "Sobre",
    aboutText: "Esta ferramenta converte um jogo de qualquer formato para qualquer outro, em qualquer plataforma, inclusive no PS5.",
    connectAny: "Como conectar de qualquer dispositivo",
    useAddress: (url) => <>Use {url} ou escaneie este código.</>,
    compare: "Comparar formatos",
    compareLabel: "Comparação de formatos",
    format: "Formato",
    upstream: (app) =>
      `Números dos autores: o que os autores do ampr_emu e do Lazy_AMPR relatam, sem medição do ${app}; variam de jogo para jogo. O ampr_emu 0.4.2.1 é uma versão de teste dos autores; problema conhecido: alguns jogos travam ao salvar.`,
    bundled: "Pastas empacotadas com LZ4 rodam no ampr_emu 0.4.2.1 de drakmor (GPL-3.0), incluído sem modificações. Código-fonte:",
    lazy: "O fluxo “rastrear e depois empacotar” segue o Lazy_AMPR de Nazky:",
    unrar:
      "Arquivos .rar são lidos com a biblioteca UnRAR incluída, de Alexander Roshal (só extração, sob a licença do UnRAR).",
    language: "Idioma",
    system: "Sistema",
  },
  // Edit.tsx
  edit: {
    details: "Detalhes",
    deleting: "Excluindo a origem…",
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
      "Abra a **pasta** ou a **imagem** de um jogo e altere-a como uma pasta num pendrive: **adicione e substitua arquivos** (arraste-os ou use “Adicionar”), **renomeie, exclua**, crie pastas.",
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
    items: (n) => `${num(n)} ${plural(n, { one: "item", many: "itens", other: "itens" })}`,
    inFolder: (dirs, files) =>
      `${num(dirs)} ${plural(dirs, { one: "pasta", many: "pastas", other: "pastas" })}, ${num(files)} ${plural(files, FILES)}`,
    matches: (n) => `${num(n)} ${plural(n, { one: "resultado", many: "resultados", other: "resultados" })}`,
    emptyFolder: "Esta pasta está vazia.",
    open: (name) => `Abrir ${name}`,
    archiveNote: "Arquivos compactados podem ser navegados, mas não editados. Para alterá-lo, converta-o antes em uma pasta ou imagem.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "Adicionar arquivos",
    addFolder: "Adicionar pasta",
    pickAddFiles: "Escolha os arquivos a adicionar",
    pickAddFolder: "Escolha uma pasta a adicionar",
    nameLabel: "Nome",
    renameItem: (name) => `Renomear ${name}`,
    removeItem: (name) => `Excluir ${name}`,
    renameTip: "Renomear (F2)",
    removeTip: "Excluir (tecla Delete)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "Extrair",
    extractItem: (name: string) => `Extrair ${name}`,
    extractTip: "Copiar para uma pasta que você escolher",
    extractFolder: "Extrair pasta",
    extractFolderTip: "Copiar a pasta exibida para uma pasta que você escolher",
    extractGameTip: "Copiar o jogo inteiro para uma pasta que você escolher",
    pickExtract: "Escolha onde extrair",
    extractPending: "Extrai os arquivos como estão agora, sem as suas alterações pendentes",
    nameEmpty: "Digite um nome.",
    nameBad: "Um nome não pode conter / ou \\, nem ser . ou ..",
    nameTaken: (name) => `${name} já existe aqui.`,
    markNew: "Novo",
    markReplaced: "Substituído",
    markRenamed: "Renomeado",
    wasAt: (path) => `Antes em ${path}`,
    sizeLater: "Mais do que foi possível listar: o restante é lido quando as alterações forem aplicadas",
    dropInto: (folder) => `Solte para adicionar em ${folder}`,
    pending: (n) =>
      plural(n, {
        one: `${num(n)} alteração ainda não aplicada`,
        many: `${num(n)} alterações ainda não aplicadas`,
        other: `${num(n)} alterações ainda não aplicadas`,
      }),
    undo: "Desfazer",
    undoTip: "Desfazer a última alteração",
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
    lz4: {
      pack: "LZ4: empacotar",
      unpack: "LZ4: desempacotar",
      trace: "LZ4: rastrear",
      unpatch: "LZ4: remover patch",
      runtime: "Runtime AMPR",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "Editar",
    editRebuild: "Editar · recriar",
    extract: "Extração",
    job: (id) => `Tarefa ${id}`,
    done: "Concluída",
    cancelled: "Cancelada",
    failed: "Falhou",
    cancelling: "Cancelando…",
    queued: "Na fila",
    dismiss: "Ocultar",
    stopJob: "Parar tarefa?",
    cancel: "Cancelar",
    stopHint: "Clique em “Parar tarefa?” de novo em até 4 segundos para pará-la.",
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
    stays: "Os resultados ficam aqui até você clicar em “Atualizar”. Novas saídas aparecem depois de atualizar.",
    library: "Biblioteca",
    storageTip: "Espaço livre no armazenamento interno do console (/data), no último escaneamento",
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
    lz4Tip: "Empacotado com LZ4: pacotes de assets do ampr_emu",
    lz4Packed: " empacotado",
  },
  // Lz4.tsx
  lz4: {
    title: "Pacotes de assets LZ4",
    help: "Como o LZ4 funciona",
    scenario: "Cenário LZ4",
    scenarios: { trace: "Rastrear", pack: "Empacotar/Desempacotar" },
    actions: { patch: "Aplicar patch", unpatch: "Remover patch", pack: "Empacotar", unpack: "Desempacotar" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "Registra quais assets o jogo lê enquanto você joga, para uso em “Empacotar”."
      : packed
        ? "Grava os pacotes LZ4 do jogo de volta como arquivos comuns."
        : "Empacota os assets do jogo em volumes LZ4 que o ampr_emu lê enquanto o jogo roda.",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "Recria esta imagem com o runtime de rastreamento e depois a substitui."
        : "Instala o runtime de rastreamento nesta pasta."
      : image
        ? "Recria esta imagem com o runtime de release e depois a substitui."
        : "Devolve o runtime de release a esta pasta.",
    noAmpr: "o jogo não usa AMPR",
    noEboot: "não há eboot.bin",
    packedFirst: "a origem está empacotada com LZ4: desempacote-a primeiro",
    damaged: "os pacotes LZ4 dela estão danificados",
    archive: "não é possível rastrear um arquivo compactado: converta-o primeiro",
    chooseFirst: "escolha uma origem primeiro",
    notRead: "a origem ainda não foi lida",
    pickProfile: "Perfil LZ4",
    profileFilter: "Perfil LZ4 (TOML)",
    pickTracesFolder: "Rastreamentos LZ4 (uma pasta com ampr_commands.bin e ampr_emu.index)",
    pickTracesEither: "Rastreamentos LZ4: o zip ou uma pasta com os dois arquivos",
    pickTracesZip: "Rastreamentos LZ4 (zip)",
    unreadable: "Não é possível iniciar: esta origem não pôde ser lida (veja “Origem”).",
    pending: (a) => a === "patch"
      ? "Não é possível aplicar o patch com tarefas em execução ou na fila."
      : "Não é possível remover o patch com tarefas em execução ou na fila.",
    replacing: "Uma tarefa já está recriando esta imagem: aguarde até ela terminar.",
    blocked: (a, first, more) =>
      `Não é possível ${a === "patch" ? "aplicar o patch" : a === "unpatch" ? "remover o patch" : a === "pack" ? "empacotar" : "desempacotar"}: ${first}${more > 0 ? ` (e mais ${num(more)}, veja “Origem”)` : ""}.`,
    chooseGame: "Escolha primeiro uma pasta de jogo ou uma imagem.",
    noAmprGame: "Este jogo não usa AMPR, então o LZ4 não pode ser usado aqui.",
    additionalContent: "Isto é conteúdo adicional (sem eboot.bin), então o LZ4 não pode ser usado aqui.",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `Este jogo não tem ampr_emu: Empacotar instala o ampr_emu ${version} incluído.`
        : a === "patch"
          ? `Este jogo não tem ampr_emu: Aplicar patch instala o runtime de rastreamento incluído (ampr_emu ${version}).`
          : `Este jogo não tem ampr_emu: Remover patch instala o runtime de release incluído (ampr_emu ${version}).`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "O ampr_emu.index não lista os arquivos deste jogo" : state === "damaged" ? "Não dá para ler o ampr_emu.index" : "Este jogo não tem ampr_emu.index"}: ${a === "pack" ? "Empacotar" : a === "patch" ? "Aplicar patch" : "Remover patch"} grava um novo.`,
    unavailable: (items) => `Indisponível para esta origem: ${items.join("; ")}.`,
    readOnly: (kind) => <>
      <b>{kind}</b> é somente leitura no console, então não pode ser rastreado. Converta-o primeiro para <b>ffpkg</b> ou <b>exfat</b>.
    </>,
    action: "Ação",
    traceAction: "Ação de rastreamento",
    changes: (a, image) => a === "patch"
      ? `Altera ${image ? "esta imagem" : "esta pasta"}: o runtime de rastreamento substitui fakelib/libSceAmpr.sprx, sem backup.`
      : `Altera ${image ? "esta imagem" : "esta pasta"}: o runtime de release volta e os rastreamentos são excluídos, sem backup.`,
    replay: (web) => `Jogue e depois escolha esta origem de novo para usar os rastreamentos dela.${web ? " “Rastrear” também os oferece como zip." : ""}`,
    room: "Espaço para o rastreamento",
    roomHint: "Espaço livre para o rastreamento do jogo. Monte a imagem com image_rw=, ou nada será gravado.",
    alreadyRelease: "Já está no runtime de release: só o índice é gravado de novo.",
    spaceHint: "Precisa de espaço livre do tamanho da imagem; ela só é substituída depois que a cópia é verificada.",
    deletesTraces: (a, bytes, web) =>
      `${a === "patch" ? "“Aplicar patch”" : "“Remover patch”"} exclui os rastreamentos desta origem (${bytes}): ${web ? "baixe-os antes." : "copie-os antes."}`,
    useProfile: "Usar um perfil de regras em vez disso",
    profile: "Perfil",
    choose: "Escolher",
    profileHint: "Um arquivo TOML com regras de empacotamento, usado no lugar dos rastreamentos.",
    target: "Destino",
    packInto: "Empacotar em",
    direct: "Empacota direto na imagem; os arquivos do jogo são lidos duas vezes.",
    targetFormat: "Formato de destino",
    unpackFolder: "Uma nova pasta do jogo com os pacotes gravados de volta como arquivos comuns.",
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
    profileRules: "Empacota o que as regras do perfil indicam; os arquivos que a engine abre diretamente ficam soltos.",
    chosenRules: "Empacota os arquivos que o jogo leu nestes rastreamentos; os arquivos que a engine abre diretamente ficam soltos.",
    ownRules: "Empacota os arquivos que o jogo leu na última sessão rastreada; os arquivos que a engine abre diretamente ficam soltos.",
    guessRules: "Sem rastreamentos: empacota por uma estimativa embutida, menos confiável. Rastreie primeiro ou escolha rastreamentos ou um perfil.",
    recorded: "Rastreamentos gravados",
    recordedHint: (bytes) => `ampr_commands.bin (${bytes}): “Empacotar” usa esses rastreamentos.`,
    downloadHint: "Feche o jogo, espere um minuto e baixe pelo navegador de um computador. Lá, escolha o zip em “Empacotar/Desempacotar” → “Rastreamentos”.",
    download: (bytes, zip) => `Baixar rastreamentos (${bytes}) como ${zip}`,
    missingIndex: "Não há ampr_emu.index nesta origem: “Empacotar” precisa dele ao lado de ampr_commands.bin.",
    truncatedJournal: "Um diário cortado no final ainda pode ser empacotado, até o último registro completo.",
    fakelibWarning: (recorded) => recorded
      ? "Poucos rastreamentos ou nenhum? Desative o atualizador da fakelib, aplique o patch de novo e jogue outra vez."
      : "Desative o atualizador da fakelib durante o rastreamento, ou nada será gravado.",
    emptyTitle: "Escolha um jogo que use AMPR",
    experimental: "Experimental: guarde uma cópia do jogo original.",
    chooseSource: "Escolher origem",
    source: "Origem",
    traces: "Rastreamentos",
    optionalTraces: "Rastreamentos (opcional)",
    unlock: "Desbloquear para escolher outros rastreamentos",
    loaded: "Os rastreamentos já foram carregados desta origem",
    lockedHint: (bytes) => `Os rastreamentos desta origem (${bytes}). Clique no cadeado para escolher outros.`,
    clear: "Limpar",
    zip: "Zip",
    folder: "Pasta",
    tracesHint: "Escolha o zip amprtrace de “Baixar rastreamentos” ou uma pasta com os dois arquivos de rastreamento.",
    useOwn: "Usar os rastreamentos da origem",
    patchResult: (a, n) =>
      a === "patch"
        ? `Patch aplicado: o runtime de rastreamento está instalado; o índice lista ${num(n)} ${plural(n, FILES)}.`
        : `Patch removido: o runtime de release voltou; o índice lista ${num(n)} ${plural(n, FILES)}.`,
    removed: (files) => `Arquivos de rastreamento antigos removidos: ${files.join(", ")}.`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "Como o LZ4 funciona",
    close: "Fechar",
    warning: "Experimental, só para jogos AMPR. Guarde uma cópia do jogo original.",
    body: () => (
      <>
        <h3>1. Empacotar/Desempacotar</h3>
        <ol>
          <li>
            Escolha o jogo original, <b>Empacotar/Desempacotar</b> e os rastreamentos (ou um perfil).
          </li>
          <li>
            Grave uma pasta empacotada ou uma imagem comum; uma pasta pode ser convertida para qualquer formato em{" "}
            <b>Converter</b>.
          </li>
          <li>
            Uma origem empacotada mostra <b>Desempacotar</b> no lugar: os arquivos soltos voltam.
          </li>
          <li>Empacotar mantém o ampr_emu do jogo se for a versão incluída ou mais nova e ler pacotes LZ4; senão, instala o de release incluído, inclusive no lugar do runtime de rastreamento do LZ4 e em um jogo que não tem um.</li>
        </ol>

        <h3>2. Rastrear</h3>
        <ol>
          <li>
            Escolha o <b>ffpkg</b> ou o <b>exfat</b> do jogo e depois <b>Rastrear</b> → <b>Aplicar patch</b>.
          </li>
          <li>Desative o atualizador da fakelib: ele sobrescreve o runtime de rastreamento.</li>
          <li>
            Monte com <span className="path">image_rw=</span>, jogue uma sessão e feche o jogo.
          </li>
          <li>
            Use <b>Baixar rastreamentos</b> na página do PS5 ou copie ampr_commands.bin e ampr_emu.index.
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
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
    chooseFiles: (n) =>
      plural(n, { one: `Escolher ${num(n)} arquivo`, many: `Escolher ${num(n)} arquivos`, other: `Escolher ${num(n)} arquivos` }),
    nothing: "Nenhuma pasta ou arquivo aqui.",
    cancel: "Cancelar",
    chooseFolder: "Escolher esta pasta",
  },
  // common.tsx
  common: {
    folder: "Pasta",
    pickFolder: "Pasta do jogo",
    pickImage: "Imagem ou arquivo compactado do jogo",
    pickImageFilter: "Imagens e arquivos compactados",
    pickFolderButton: "Pasta",
    pickImageButton: "Imagem/Arquivo compactado",
    source: "Origem",
    emptyTitle: "Escolha uma pasta, imagem ou arquivo compactado de jogo",
    emptyText: "Uma pasta de jogo de PS5 ou uma imagem, pacote ou arquivo compactado dela.",
    accepted: "Origens aceitas",
    unknownTitle: "Título desconhecido",
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
      forge_trace: "de rastreamento do Forge",
      other: "outro",
      none: "nenhum",
    },
    packed: "Empacotado",
    damaged: "Pacotes danificados",
    traced: "Rastreado",
    plain: "Não empacotado",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} ${plural(files, FILES)} em ${num(volumes)} ${plural(volumes, { one: "volume", many: "volumes", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)}% do tamanho`),
    journal: (size) => `diário de ${size}`,
    noJournal: "ainda sem diário",
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
        : "O arquivo de imagem será excluído para sempre: sem Lixeira, sem desfazer. Se a exclusão parar no meio, parte dele pode já ter sido apagada.",
    no: "Não",
    yes: "Sim, excluir",
    archiveTitle: "Excluir este arquivo compactado?",
    archiveText:
      "O arquivo compactado será excluído para sempre: sem Lixeira, sem desfazer. As outras partes de um arquivo em várias partes ficam como estão.",
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
        tone: "good",
        text: "Recomendado. O sistema de arquivos próprio do PS5 (UFS2). Permite montagem com gravação.",
      },
      ffpfs: {
        tone: "warn",
        text: "Experimental. O sistema de arquivos PFS do PS5, sem compressão. Os nomes de arquivo precisam ser ASCII simples.",
      },
      ffpfsc: {
        tone: "warn",
        text: "Experimental. O menor: um contêiner comprimido em torno de uma imagem, sempre montado como somente leitura.",
      },
      pkg: {
        text: "Instala como um jogo da loja e roda na velocidade nativa. Requer kstuff, fpkg-enable e ppr-patch.",
      },
      lz4: {
        tone: "warn",
        text: "Experimental. Uma pasta de jogo com os assets empacotados em volumes LZ4, que o emulador AMPR incluído (ampr_emu) desempacota enquanto o jogo roda. Só para jogos que usam AMPR.",
      },
    },
    innerFfpkg: { tone: "good", text: "Recomendado. O sistema de arquivos próprio do PS5 (UFS2)." },
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
    pkgFirmware: "Funciona só no firmware 11.60 ou anterior.",
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
    updatingNote: "A nova versão está sendo baixada, o app vai reiniciar sozinho.",
    downloading: "Baixando no navegador: descompacte o arquivo compactado e substitua esta cópia pelo conteúdo dele.",
    failed: (v) => `Não foi possível atualizar para a v${v}`,
    timeout: (v) => `A v${v} não iniciou em um minuto. Se ela foi baixada, pare o Dump Forge e abra-o de novo para concluir a atualização.`,
    retry: "Tentar de novo",
    close: "Fechar",
  },
  // api.ts: prettyDuration's units
  duration: { s: (s) => `${s} s`, ms: (m, s) => `${m} min ${s} s`, hm: (h, m) => `${h} h ${m} min` },
};

export default pt;
