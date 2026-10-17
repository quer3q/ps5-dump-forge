// Simplified Chinese (mainland). Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Chinese has one plural form (CLDR `other`), so counts need no `plural`.
// Terms: trace 追踪 (traces 追踪数据), pack/unpack 打包/解包, patch/unpatch 打补丁/移除补丁,
// runtime 运行时, image 镜像, source 源, output 输出, build 构建, verify 校验, inspect 检查.

import { dotted, num } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "与游戏大小相同";

const zh: Messages = {
  // App.tsx
  app: {
    address: (url) => `地址：${url}。查看连接方法和 QR 码`,
    screens: "页面",
    convert: "转换",
    active: (n) => `，${num(n)} 个进行中`,
    edit: "编辑",
    library: "游戏库",
    about: "关于",
    offline: "已离线，正在重试…",
    stop: "停止 Dump Forge",
    stillRunning: (n) => (n === 1 ? "仍有一个任务在运行" : `仍有 ${num(n)} 个任务在运行`),
    job: (id) => `任务 ${id}`,
    quitNote: (web, n) =>
      `${web ? "停止 Dump Forge" : "退出"}会中止${n === 1 ? "该任务" : "这些任务"}，并删除未完成的输出。`,
    keepRunning: "继续运行",
    stopAndQuit: (n) => (n === 1 ? "停止任务并退出" : "停止所有任务并退出"),
    stoppingJobs: "正在停止任务并清理…",
    stoppingForge: "正在停止 Dump Forge…",
    couldntStop: "无法停止 Dump Forge",
    close: "关闭",
    tryAgain: "重试",
    stopped: "Dump Forge 已停止",
    startAgain: (ps5) => (ps5 ? "如需继续，请再次打开 PS5 上的 Dump Forge 磁贴。" : "如需继续，请再次运行同一条 ps5-dump-forge serve 命令。"),
    confirmStop: (web: boolean) => (web ? "停止 Dump Forge？" : "退出 PS5 Dump Forge？"),
    stopNote: (web, ps5) => (!web ? "没有正在运行的任务。" : ps5 ? "如需再次启动，请打开 PS5 上的 Dump Forge 磁贴。" : "如需再次启动，请重新运行同一条 ps5-dump-forge serve 命令。"),
    quit: "退出",
    github: (name) => `GitHub 上的 ${name}`,
    queued: "排队中",
    progress: (percent, left) => `已完成 ${percent}%${left ? `，剩余 ${left}` : ""}`,
  },
  // Convert.tsx
  convert: {
    target: "目标",
    targetFormat: "目标格式",
    alreadyFolder: "源已经是文件夹",
    lz4Backport:
      "此游戏的资源已用 LZ4 打包，且其 LZ4 清单包含向下移植文件：请先在“LZ4”选项卡中解包，再在此处移除向下移植。",
    unreadable: "无法构建：无法读取此源（见“源”）。",
    noContentId: "无法构建为 fpkg：此游戏的 param.json 没有软件包所需的 Content ID。",
    blocked: (first, more) =>
      `无法构建：${first}${more > 0 ? `（另有 ${num(more)} 项，见“源”）` : ""}。`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `此游戏使用 AMPR 但没有 ampr_emu：转换时会安装内置的 ampr_emu ${version}。`;
      if (a === "trace") return `转换时会用内置的正式版 ampr_emu ${version} 替换 LZ4 追踪运行时。`;
      if (a === "replace")
        return own
          ? `转换时会用内置的 ampr_emu ${version} 替换此游戏的 ampr_emu ${own}。`
          : `转换时会用内置的 ampr_emu ${version} 替换此游戏未标明版本的 ampr_emu。`;
      return own
        ? `此游戏的 ampr_emu ${own} 不低于内置的 ${version}：转换时会保留它。`
        : `此游戏已带有内置的 ampr_emu ${version}：转换时会保留它。`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "ampr_emu.index 与此游戏的文件不符" : state === "damaged" ? "无法读取 ampr_emu.index" : "此游戏没有 ampr_emu.index"}：转换时会写入新的索引${traces ? "，并略去旧的追踪数据" : ""}。`,
    amprPkgIndex: (traces) =>
      `软件包会获得根据其中文件新建的 ampr_emu.index${traces ? "，并略去旧的追踪数据" : ""}。`,
    removeBackport: "移除向下移植",
    removeHint: (libs, emulators, other) =>
      `从 fakelib/ 中排除 ${num(libs)} 个向下移植库` +
      (emulators.length === 0
        ? other
          ? "；其他自制库保留。"
          : "。"
        : `；模拟器（${emulators.join("、")}）${other ? "及其他自制库" : ""}保留。`),
    cantRemove: (why) => `无法移除向下移植：${why}`,
    compare: "比较格式",
    build: "构建",
    likelyFail: (fmt) => (
      <>
        构建为 <b>{fmt}</b> 很可能失败：该格式不接受“源”中列出的文件名。
      </>
    ),
    collides: "已有任务正在写入此文件：再次构建会失败。请换一个名称。",
    already: (fmt) => (
      <>
        已有任务正在将此游戏构建为 <b>{fmt}</b>；再次构建会生成第二份副本。
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "优点", ok: "折中", bad: "缺点" },
    columns: ["主机上的速度", "占用空间"],
    rows: {
      pkg: {
        tagline: "最佳，前提是固件允许",
        cells: ["原生，全速", "减少 30–60%，视游戏而定"],
      },
      lz4: {
        tagline: "实验性，适用于使用 AMPR 的游戏",
        cells: ["加载快 30–40%（上游数据）", "约为游戏大小的一半（上游数据）"],
      },
      ffpkg: { tagline: "ShadowMountPlus 推荐格式", cells: ["镜像全速", FLAT] },
      exfat: { tagline: "兼容性", cells: ["镜像全速", FLAT] },
      ffpfs: { tagline: "实验性", cells: ["镜像全速", FLAT] },
      ffpfsc: {
        tagline: "体积最小，实验性",
        cells: ["150–250 MB/s，可能卡顿", "取决于压缩级别，通常减少 30–60%"],
      },
      folder: { tagline: "普通游戏文件", cells: ["硬盘全速", FLAT] },
    },
    features: [
      ["转换", "可在文件夹、ffpkg、exfat、ffpfs、ffpfsc 与 fpkg 之间双向进行。"],
      ["编辑", "像操作文件夹一样修改游戏的文件，并在构建前查看其封面、固件、向下移植、DLC 和文件名问题。"],
      ["删除", "经您确认后，将游戏从磁盘上移除。"],
      ["LZ4", "追踪 AMPR 游戏，再以智能修复方式打包其资源（实验性）。"],
      ["流式", "可从任意来源到任意目标：无需临时副本，即使是 ffpfsc 内的镜像或压缩包中的游戏。"],
      ["压缩包", "包含一款游戏的 .zip、.7z 或 .rar 可像文件夹一样浏览、转换或打包。"],
      ["游戏库", "这台电脑或主机上的游戏，可直接转换和编辑。"],
      ["安全", "除非您要求，否则绝不修改您的游戏；会跳过垃圾文件，并提前报告不合规的文件名。"],
      ["USB 和 SD 卡", "可写入 exFAT 或 FAT32 格式的 U 盘和存储卡。"],
      ["原位或重建", "能容纳时直接在镜像内编辑，否则生成新镜像，校验通过后替换原镜像。"],
    ],
    howToConnect: "连接方法",
    connectText: (url) => <>在本地网络中任意设备的浏览器里打开 {url}，或扫描此 QR 码。</>,
    close: "关闭",
    about: "关于",
    aboutText: "本工具可在任何平台（包括 PS5）上将游戏从任意格式转换为其他任意格式。",
    connectAny: "如何从任意设备连接",
    useAddress: (url) => <>打开 {url} 或扫描此 QR 码。</>,
    compare: "比较格式",
    compareLabel: "格式比较",
    format: "格式",
    upstream: (app) =>
      `上游数据：来自 ampr_emu 和 Lazy_AMPR 作者的报告，并非 ${app} 实测；具体因游戏而异。ampr_emu 0.4.2.1 是上游的测试版本；已知问题：部分游戏在存档时崩溃。`,
    bundled: "LZ4 打包后的游戏使用 drakmor 的 ampr_emu 0.4.2.1（GPL-3.0）运行；该运行时原样内置，未作修改。源代码：",
    lazy: "“先追踪、后打包”的流程沿用 Nazky 的 Lazy_AMPR：",
    unrar:
      ".rar 压缩包由内置的 UnRAR 库（作者 Alexander Roshal）读取（仅解压，遵循 UnRAR 许可）。",
    language: "语言",
    system: "跟随系统",
  },
  // Edit.tsx
  edit: {
    details: "详情",
    deleting: "正在删除源…",
    delete: "删除源",
    deleted: "已删除：",
    filter: "按路径筛选",
    filterLabel: "按路径筛选文件",
    firstOf: (max, n) => `前 ${num(max)} 个，共 ${num(n)} 个`,
    fileCount: (n) => `${num(n)} 个文件`,
    noMatch: (f) => `没有与“${f}”匹配的文件。`,
    noListing: "不支持显示压缩包的文件列表。",
    noParam: "没有 sce_sys/param.json。",
    reader: "读取器",
    leftovers: "残留的文件和文件夹",
    found: (n) => `找到 ${num(n)} 个`,
    leftoversNote: (app) =>
      `未完成的输出，通常来自被中断的任务。只删除您认得的项目，并且只在没有任务运行时删除；${app} 本身从不删除它们。`,
    checked: (n) => `已检查的文件夹（${num(n)}）`,
    /** Edit's folds under the game card. */
    facts: "游戏信息",
    explorer: "游戏文件",
    howTitle: "使用说明",
    /** The empty state's paragraphs. */
    how: (app) => [
      "打开**游戏文件夹**或**镜像**，然后像操作 U 盘里的文件夹一样修改它：**添加和替换文件**（拖进来，或使用“添加”），**重命名、删除**、新建文件夹。",
      `**在您点击“应用”之前**不会有任何改动：您的修改会在这里逐项累积，然后 ${app} 一次性将它们直接写入镜像或文件夹，并检查结果。这**不是挂载的磁盘**：${app} 会记录您的修改并自行应用。`,
      `修改放得下时，会**直接写入镜像**；放不下时（镜像内没有剩余空间，或格式无法接受这些修改），${app} 会在旁边构建一个包含您修改的新镜像，检查无误后再替换原镜像。无论哪种方式，一旦出错，镜像都会保持原样。`,
      "压缩包可以浏览，但不能编辑。",
    ],
    location: "游戏中的位置",
    root: "游戏",
    up: "上级",
    upTip: "返回上一级文件夹",
    name: "名称",
    size: "大小",
    items: (n) => `${num(n)} 项`,
    inFolder: (dirs, files) => `${num(dirs)} 个文件夹，${num(files)} 个文件`,
    matches: (n) => `${num(n)} 个匹配项`,
    emptyFolder: "此文件夹为空。",
    open: (name) => `打开 ${name}`,
    archiveNote: "压缩包可以浏览，但不能编辑。如需修改，请先将其转换为文件夹或镜像。",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "添加文件",
    addFolder: "添加文件夹",
    pickAddFiles: "选择要添加的文件",
    pickAddFolder: "选择要添加的文件夹",
    nameLabel: "名称",
    renameItem: (name) => `重命名 ${name}`,
    removeItem: (name) => `删除 ${name}`,
    renameTip: "重命名（F2）",
    removeTip: "删除（Delete 键）",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "解压",
    extractItem: (name: string) => `解压 ${name}`,
    extractTip: "复制到您选择的文件夹",
    extractFolder: "解压文件夹",
    extractFolderTip: "将当前显示的文件夹复制到您选择的文件夹",
    extractGameTip: "将整个游戏复制到您选择的文件夹",
    pickExtract: "选择解压位置",
    extractPending: "按当前状态解压文件，不包含您尚未应用的修改",
    nameEmpty: "请输入名称。",
    nameBad: "名称不能包含 / 或 \\，也不能是 . 或 ..",
    nameTaken: (name) => `此处已存在 ${name}。`,
    markNew: "新增",
    markReplaced: "已替换",
    markRenamed: "已重命名",
    wasAt: (path) => `原位置：${path}`,
    sizeLater: "内容过多，无法全部列出：其余部分将在应用修改时读取",
    dropInto: (folder) => `拖放以添加到 ${folder}`,
    pending: (n) => `${num(n)} 项修改尚未应用`,
    undo: "撤销",
    undoTip: "撤销上一项修改",
    discard: "全部放弃",
    discardTip: "放弃所有尚未应用的修改",
    discardTitle: (n) => (n === 1 ? `放弃 ${num(n)} 项修改？` : `放弃全部 ${num(n)} 项修改？`),
    discardText: "这些修改将被丢弃，游戏不会有任何变化。此操作无法撤销。",
    discardNo: "否",
    changesList: "修改（按顺序）",
    willNotApply: (n) => `${num(n)} 项修改无法按现状应用。请撤销，或全部放弃。`,
    opAdd: (to) => <>添加 {to}</>,
    opAddFolder: (to) => <>添加文件夹 {to}</>,
    opDelete: (path) => <>删除 {path}</>,
    opRename: (from, to) => <>将 {from} 重命名为 {to}</>,
    opMkdir: (path) => <>新建文件夹 {path}</>,
    opFrom: (src) => <>来自 {src}</>,
    whyInvalid: (path) => <>{path} 不是有效的游戏路径。</>,
    whyMissing: (path) => <>游戏中没有 {path}。</>,
    whyExists: (path) => <>{path} 已存在。</>,
    whyFile: (path) => <>{path} 是文件。</>,
    whyFolder: (path) => <>{path} 是文件夹。</>,
    whyInto: (path) => <>{path} 不能移动到它自身之中。</>,
    conflictTitle: (n) => `此处已存在 ${num(n)} 个项目`,
    conflictIn: (folder) => <>位于 {folder}：</>,
    conflictText:
      "“替换”会用新项目取代它们：修改应用后，旧内容将永久消失，而有问题的文件可能导致游戏无法启动。添加到同名文件夹上的文件夹会被合并，其中同名的文件会被替换。",
    replace: "替换",
    skip: "跳过",
    cancel: "取消",
    /** How the changes go in (the pending bar), from core's check before Apply. */
    modeLabel: "应用方式",
    checking: "正在检查修改的应用方式",
    checkFailed: "无法检查修改：",
    checkAgain: "重新检查",
    inPlace: "原地修改",
    rebuild: "重建",
    rebuildNeeded: "需要重建",
    cantApply: "无法应用",
    changedDirectly: "直接修改",
    folderText: "直接写入文件夹。",
    inPlaceText: "写入镜像。",
    inPlaceNeeds: (need, free) => `写入镜像；需要 ${need}，镜像内可用 ${free}。`,
    noRoom: (need, free) => `这些修改需要 ${need}，而镜像内只有 ${free} 可用空间。`,
    alwaysRebuilt: "ffpfsc 和 fpkg 镜像总是重建。",
    rebuildText: "在旁边写入一个新镜像，校验通过后替换原镜像。",
    rebuildNeeds: (need, free) => `在旁边写入一个新镜像（约需 ${need} 可用空间，当前可用 ${free}），校验通过后替换原镜像。`,
    uploadLater: (n) => `来自此浏览器的 ${num(n)} 个文件将在您点击“应用”并上传完成后检查。`,
    decidedOnApply: (n) =>
      n === 1
        ? "其他修改依赖来自此浏览器的这个文件，因此它们的应用方式将在您点击“应用”并上传完成后决定。"
        : `其他修改依赖来自此浏览器的这 ${num(n)} 个文件，因此它们的应用方式将在您点击“应用”并上传完成后决定。`,
    sourceLocked: "正在向此游戏应用修改。请在任务结束后再选择其他源。",
    notOpened: (path) => <>未打开 {path}：正在向此游戏应用修改。请在任务结束后重新打开。</>,
    apply: "应用",
    applyTip: "应用此处列出的所有修改",
    applyTitle: (n) => `应用 ${num(n)} 项修改？`,
    applyInPlace: (name) => <>这些修改将直接写入 {name}。完成后无法撤销。</>,
    applyRebuild: (name) => <>将在 {name} 旁边写入包含您修改的新镜像，校验后放到原镜像的位置。替换完成后无法撤销。</>,
    closeImage: "如果游戏正从此镜像运行，请先关闭游戏。",
    closeFolder: "如果游戏正从此文件夹运行，请先关闭游戏。",
    applyRestores: "此镜像上一次的编辑被中断：会先将镜像恢复原状。",
    uploading: (i, n, sent, total) => `正在上传第 ${num(i)} 个文件（共 ${num(n)} 个）：${sent} / ${total}`,
    uploadLabel: "上传",
    uploadCancelled: "上传已取消。游戏没有任何变化。",
    applyRunning: "正在应用修改。下方任务结束前无法继续编辑。",
    applyFailed: "修改未能应用：",
    applyRefused: "无法以这种方式应用修改：",
    applyCancelled: "任务已取消。您的修改仍列在这里。",
    folderPartly: "部分修改可能已写入文件夹：显示的文件就是文件夹当前的内容。",
    interrupted: "此镜像上一次的编辑被中断。在检查或应用您接下来的修改之前，会先将镜像恢复原状。",
  },
  // JobList.tsx
  jobs: {
    jobs: "任务",
    pending: (n) => `${num(n)} 个运行中或排队中`,
    allFinished: "全部完成",
    stages: {
      scan: "正在读取源",
      preflight: "正在检查",
      check: "正在检查",
      plan: "正在规划",
      compress: "正在压缩",
      write: "正在写入",
      measure: "正在测算包",
      pack: "正在打包",
      verify: "正在校验",
      finalize: "正在收尾",
    },
    steps: {
      scan: "读取",
      preflight: "检查",
      check: "检查",
      plan: "规划",
      compress: "压缩",
      write: "写入",
      measure: "测算",
      pack: "打包",
      verify: "校验",
      finalize: "收尾",
    },
    stepsLabel: "步骤",
    stepDone: (s) => `${s}（已完成）`,
    stepLater: (s) => `${s}（未开始）`,
    packingWriting: "正在打包并写入",
    rate: { read: "读取", compress: "压缩", pack: "打包", write: "写入" },
    showPath: "显示路径",
    showFinder: "在访达中显示",
    showExplorer: "在资源管理器中显示",
    showFolder: "在文件夹中显示",
    lz4: {
      pack: "LZ4 打包",
      unpack: "LZ4 解包",
      trace: "LZ4 追踪",
      unpatch: "LZ4 移除补丁",
      runtime: "AMPR 运行时",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "编辑",
    editRebuild: "编辑 · 重建",
    extract: "解压",
    job: (id) => `任务 ${id}`,
    done: "完成",
    cancelled: "已取消",
    failed: "失败",
    cancelling: "正在取消…",
    queued: "排队中",
    dismiss: "移除",
    stopJob: "停止任务？",
    cancel: "取消",
    stopHint: "请在 4 秒内再次点击“停止任务？”以停止它。",
    from: "来源：",
    left: (d) => `剩余 ${d}`,
    inStep: (time) => `本步骤已用 ${time}`,
    result: (bytes, n) => `${bytes}，${num(n)} 个文件 →`,
    outputPath: "输出路径",
    checks: "检查项",
    verifiedPlain: (n) => `已校验：${num(n)} 项检查通过`,
    verifiedFull: (n) => `完整校验通过：${num(n)} 项检查`,
    verifiedFast: (n) => `快速校验通过：${num(n)} 项检查`,
    coverage: (checked, total, n) => `抽样 ${num(n)} 次，共核对 ${checked}（总计 ${total}）`,
    cantBuild: "无法构建此游戏：",
    cantEdit: "无法应用修改：",
    cantExtract: "无法解压：",
    cantBuildAs: (f) => (
      <>
        无法将此游戏构建为 <b>{f}</b>：
      </>
    ),
    log: (n) => `日志（${num(n)}）`,
    logLabel: "日志",
  },
  // Library.tsx
  library: {
    pickTitle: "选择游戏库文件夹",
    scanningIn: (name) => `正在扫描 ${name}`,
    scanning: "正在扫描",
    cantScanFolder: "无法扫描此文件夹",
    chooseAnother: "选择其他文件夹",
    pickPrompt: "选择存放游戏的文件夹进行扫描",
    choose: "选择文件夹",
    cantScan: "无法扫描游戏库：",
    cantDelete: "无法删除：",
    deleted: (path) => `已删除 ${path}`,
    truncated: "已达到扫描上限。此列表可能不完整。",
    limits: "搜索最多深入六层文件夹，扫描约 15 秒后停止。",
    noRoots: "没有可扫描的文件夹。",
    none: "未找到游戏。搜索最多深入六层并跳过系统文件夹。其他源仍可在“编辑”或“转换”中选择。",
    skipped: "已跳过的位置和文件",
    stays: "结果会保留到您点击“刷新”为止。新的输出在刷新后显示。",
    library: "游戏库",
    storageTip: "主机内部存储（/data）的可用空间，截至上次扫描",
    free: (free, total) => (
      <>
        可用 <b>{free}</b>，共 {total}
      </>
    ),
    refresh: "刷新",
    cards: "卡片视图",
    list: "列表视图",
    games: "游戏",
    edit: (n) => `编辑 ${n}`,
    convert: (n) => `转换 ${n}`,
    remove: (n) => `删除 ${n}`,
    lz4Tip: "已用 LZ4 打包：ampr_emu 资源包",
    lz4Packed: " 已打包",
  },
  // Lz4.tsx
  lz4: {
    title: "LZ4 资源包",
    help: "LZ4 使用说明",
    scenario: "LZ4 场景",
    scenarios: { trace: "追踪", pack: "打包/解包" },
    actions: { patch: "打补丁", unpatch: "移除补丁", pack: "打包", unpack: "解包" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "记录游戏在您游玩时读取的资源，供打包使用。"
      : packed
        ? "将游戏的 LZ4 包还原为普通文件。"
        : "将游戏的资源打包为 LZ4 卷，由 ampr_emu 在游戏运行时读取。",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "用追踪运行时重新构建此镜像，然后替换原镜像。"
        : "在此文件夹中安装追踪运行时。"
      : image
        ? "用正式版运行时重新构建此镜像，然后替换原镜像。"
        : "在此文件夹中恢复正式版运行时。",
    noAmpr: "该游戏未使用 AMPR",
    noEboot: "没有 eboot.bin",
    packedFirst: "源已用 LZ4 打包：请先解包",
    damaged: "其 LZ4 包已损坏",
    archive: "压缩包无法追踪：请先转换",
    chooseFirst: "请先选择源",
    notRead: "尚未读取源",
    pickProfile: "LZ4 配置文件",
    profileFilter: "LZ4 配置文件（TOML）",
    pickTracesFolder: "LZ4 追踪数据（包含 ampr_commands.bin 和 ampr_emu.index 的文件夹）",
    pickTracesEither: "LZ4 追踪数据：zip 文件，或包含这两个文件的文件夹",
    pickTracesZip: "LZ4 追踪数据（zip）",
    unreadable: "无法开始：无法读取此源（见“源”）。",
    pending: (a) => a === "patch"
      ? "有任务正在运行或排队时无法打补丁。"
      : "有任务正在运行或排队时无法移除补丁。",
    replacing: "已有任务正在重新构建此镜像：请等待其完成。",
    blocked: (a, first, more) =>
      `无法${a === "patch" ? "打补丁" : a === "unpatch" ? "移除补丁" : a === "pack" ? "打包" : "解包"}：${first}${more > 0 ? `（另有 ${num(more)} 项，见“源”）` : ""}。`,
    chooseGame: "请先选择游戏文件夹或镜像。",
    noAmprGame: "该游戏未使用 AMPR，因此这里无法使用 LZ4。",
    additionalContent: "这是附加内容（没有 eboot.bin），因此这里无法使用 LZ4。",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `该游戏没有 ampr_emu：打包时会安装内置的 ampr_emu ${version}。`
        : a === "patch"
          ? `该游戏没有 ampr_emu：打补丁时会安装内置的追踪运行时（ampr_emu ${version}）。`
          : `该游戏没有 ampr_emu：移除补丁时会安装内置的正式版运行时（ampr_emu ${version}）。`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "ampr_emu.index 与此游戏的文件不符" : state === "damaged" ? "无法读取 ampr_emu.index" : "此游戏没有 ampr_emu.index"}：${a === "pack" ? "打包" : a === "patch" ? "打补丁" : "移除补丁"}时会写入新的索引。`,
    unavailable: (items) => `不适用于此源：${items.join("；")}。`,
    readOnly: (kind) => <>
      <b>{kind}</b> 在主机上是只读的，因此无法追踪。请先将其转换为 <b>ffpkg</b> 或 <b>exfat</b>。
    </>,
    action: "操作",
    traceAction: "追踪操作",
    changes: (a, image) => a === "patch"
      ? `将修改此${image ? "镜像" : "文件夹"}：追踪运行时替换 fakelib/libSceAmpr.sprx，不保留备份。`
      : `将修改此${image ? "镜像" : "文件夹"}：恢复正式版运行时并删除追踪数据，不保留备份。`,
    replay: (web) => `游玩后再次选择此源，即可使用其追踪数据。${web ? "“追踪”还可将其打包为 zip 提供下载。" : ""}`,
    room: "追踪数据空间",
    roomHint: "供游戏写入追踪数据的可用空间。请用 image_rw= 挂载镜像，否则不会记录任何内容。",
    alreadyRelease: "已是正式版运行时：只会重新写入索引。",
    spaceHint: "需要与镜像大小相当的可用空间；副本校验通过后才会替换镜像。",
    deletesTraces: (a, bytes, web) =>
      `${a === "patch" ? "打补丁" : "移除补丁"}会删除此源的追踪数据（${bytes}）：${web ? "请先下载。" : "请先复制一份。"}`,
    useProfile: "改用规则配置文件",
    profile: "配置文件",
    choose: "选择",
    profileHint: "包含打包规则的 TOML 文件，用于代替追踪数据。",
    target: "目标",
    packInto: "打包到",
    direct: "直接打包到镜像中；游戏文件会读取两次。",
    targetFormat: "目标格式",
    unpackFolder: "新的游戏文件夹，其中的包已还原为普通文件。",
    unpackHint: (n) =>
      n === null ? "将已打包的文件还原为普通文件。" : `将 ${num(n)} 个已打包的文件还原为普通文件。`,
    likelyFail: (fmt) => <>
      输出为 <b>{fmt}</b> 很可能失败：该格式不接受“源”中列出的文件名。
    </>,
    collides: (a, folder) =>
      `已有任务正在写入此${folder ? "文件夹" : "文件"}：${a === "pack" ? "打包" : "解包"}需要换一个名称。`,
    already: (a, fmt) => <>
      已有任务正在将此游戏输出为 <b>{fmt}</b>；再次{a === "pack" ? "打包" : "解包"}会生成第二份副本。
    </>,
    profileRules: "按配置文件的规则打包；引擎直接打开的文件保持不打包。",
    chosenRules: "打包游戏在这些追踪数据中读取过的文件；引擎直接打开的文件保持不打包。",
    ownRules: "打包游戏在上一次追踪会话中读取过的文件；引擎直接打开的文件保持不打包。",
    guessRules: "没有追踪数据：将按可靠性较低的内置推测打包。请先追踪，或选择追踪数据或配置文件。",
    recorded: "已记录的追踪数据",
    recordedHint: (bytes) => `ampr_commands.bin（${bytes}）：“打包”会使用这些追踪数据。`,
    downloadHint: "关闭游戏并等待约一分钟，然后在电脑浏览器中下载。再在电脑上的“打包/解包”→“追踪数据”中选择该 zip。",
    download: (bytes, zip) => `下载追踪数据（${bytes}），保存为 ${zip}`,
    missingIndex: "此源中没有 ampr_emu.index：打包需要它与 ampr_commands.bin 放在一起。",
    truncatedJournal: "末尾被截断的记录仍可打包，处理到最后一条完整记录为止。",
    fakelibWarning: (recorded) => recorded
      ? "追踪数据很少或没有？请关闭 fakelib 更新程序，重新打补丁后再玩一次。"
      : "追踪期间请关闭 fakelib 更新程序，否则不会记录任何内容。",
    emptyTitle: "请选择使用 AMPR 的游戏",
    experimental: "实验性：请保留原始游戏的副本。",
    chooseSource: "选择源",
    source: "源",
    traces: "追踪数据",
    optionalTraces: "追踪数据（可选）",
    unlock: "解锁以选择其他追踪数据",
    loaded: "已从此源加载追踪数据",
    lockedHint: (bytes) => `此源自带的追踪数据（${bytes}）。点击锁形图标可改选其他追踪数据。`,
    clear: "清除",
    zip: "Zip",
    folder: "文件夹",
    tracesHint: "选择通过“下载追踪数据”得到的 amprtrace zip，或包含两个追踪文件的文件夹。",
    useOwn: "使用源自带的追踪数据",
    patchResult: (a, n) =>
      `${a === "patch" ? "已打补丁：追踪运行时已安装" : "已移除补丁：正式版运行时已恢复"}；索引列出了 ${num(n)} 个文件。`,
    removed: (files) => `已删除旧的追踪文件：${files.join("、")}。`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "LZ4 使用说明",
    close: "关闭",
    warning: "实验性，仅适用于 AMPR 游戏。请保留原始游戏的副本。",
    body: () => (
      <>
        <h3>1. 打包/解包</h3>
        <ol>
          <li>
            选择原始游戏、<b>打包/解包</b>，以及追踪数据（或配置文件）。
          </li>
          <li>输出已打包的文件夹或普通镜像；文件夹可在 <b>转换</b> 中转换为任意格式。</li>
          <li>
            对于已打包的源，这里显示的是 <b>解包</b>：文件会还原为普通文件。
          </li>
          <li>如果游戏自带的 ampr_emu 不低于内置版本且能读取 LZ4 包，打包会保留它；否则安装内置的正式版 ampr_emu；LZ4 追踪运行时也会被替换，没有 ampr_emu 的游戏也一样。</li>
        </ol>

        <h3>2. 追踪</h3>
        <ol>
          <li>
            选择游戏的 <b>ffpkg</b> 或 <b>exfat</b>，然后依次选择 <b>追踪</b> → <b>打补丁</b>。
          </li>
          <li>关闭 fakelib 更新程序：它会覆盖追踪运行时。</li>
          <li>
            用 <span className="path">image_rw=</span> 挂载镜像，玩一次，然后关闭游戏。
          </li>
          <li>
            在 PS5 页面上点击 <b>下载追踪数据</b>，或复制 ampr_commands.bin 和 ampr_emu.index。
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    drives: "驱动器",
    location: "位置",
    parent: "上级文件夹",
    allDrives: "所有驱动器",
    truncated: (n) => `仅显示前 ${num(n)} 项。`,
    loading: "正在加载",
    noDrives: "未找到驱动器。",
    noFolders: "此处没有文件夹。",
    noImages: "此处没有文件夹、PS5 镜像或压缩包。",
    noFiles: (ext) => `此处没有文件夹或 ${dotted(ext, " 或 ")} 文件。`,
    chooseFiles: (n) => `选择 ${num(n)} 个文件`,
    nothing: "此处没有文件夹或文件。",
    cancel: "取消",
    chooseFolder: "选择此文件夹",
  },
  // common.tsx
  common: {
    folder: "文件夹",
    pickFolder: "游戏文件夹",
    pickImage: "游戏镜像或压缩包",
    pickImageFilter: "镜像和压缩包",
    pickFolderButton: "文件夹",
    pickImageButton: "镜像/压缩包",
    source: "源",
    emptyTitle: "请选择游戏文件夹、镜像或压缩包",
    emptyText: "PS5 游戏文件夹，或其镜像、安装包或压缩包。",
    accepted: "支持的源",
    unknownTitle: "未知游戏",
    additionalContent: "附加内容",
    reading: "正在读取源",
    unreadable: "无法读取的源",
    backport: "向下移植",
    emulators: "模拟器",
    declares: (fw) => `声明 ${fw}`,
    fwUnknown: "无法从可执行文件得知固件（没有可读的 SDK）",
    emulatorList: (names) => `模拟器：${names}`,
    otherEmulator: "其他",
    sdk: (v) => `SDK ${v}（声明）`,
    titleId: "游戏 ID",
    version: "版本",
    firmware: "固件",
    fwDeclared: "声明",
    fwBackport: "向下移植",
    fwExecutables: "可执行文件",
    size: "大小",
    files: (n) => `${num(n)} 个文件`,
    contentId: "内容 ID",
    noAmpr: "eboot.bin 未导入 libSceAmpr",
    runtime: {
      forge_release: "Forge 正式版",
      forge_trace: "Forge 追踪版",
      other: "其他",
      none: "无",
    },
    packed: "已打包",
    damaged: "包已损坏",
    traced: "已追踪",
    plain: "未打包",
    packedDetail: (files, volumes, percent) =>
      `${num(files)} 个文件，位于 ${num(volumes)} 个卷中` + (percent === null ? "" : `，为原大小的 ${num(percent)}%`),
    journal: (size) => `记录 ${size}`,
    noJournal: "尚无记录",
    runtimeIs: (label) => `运行时：${label}`,
    index: {
      stale: "ampr_emu.index 已过期：转换时会更新",
      damaged: "ampr_emu.index 已损坏：转换时会重写",
      missing: "没有 ampr_emu.index：转换时会写入",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `转换时会安装内置的 ampr_emu ${version}`
        : a === "replace"
          ? `转换时会用内置的 ampr_emu ${version} 替换运行时`
          : `ampr_emu ${version}：转换时会保留`,
    backportLibs: (exclusive, n) =>
      `${exclusive ? "fakelib2/，单独挂载" : "fakelib/"}，${num(n)} 个向下移植库`,
    findings: "预检发现的问题",
    dlcTitle: (n) => `内置 DLC（${num(n)}）`,
    dlcLabel: "内置 DLC",
    dlcAll: (note) => `全部${note}。`,
    dlcIn: (folder, size) => `位于 ${folder}/，${size}`,
    dlcNoData: "由 DLC 模拟器（dlc_emu.ini）解锁：仅授权，没有自己的数据",
    dlcStatus: (status) => `由 DLC 模拟器（dlc_emu.ini）解锁，状态：${status || "未设置"}`,
    dlcMerged: "已合并到游戏文件夹中",
    backportFiles: "向下移植文件",
    emulatorsIn: "fakelib 中的模拟器",
    qr: (url) => `QR 码：${url}`,
    needsPassword: "此压缩包需要密码。",
    enterPassword: "输入密码",
    inArchive: (ext, size) => `位于 ${ext} 压缩包中 · ${size}`,
    archive: "压缩包",
    parts: (n) => `${num(n)} 个分卷`,
    archiveRoot: (root) => `游戏位于 ${root}`,
    archiveAtRoot: "游戏位于压缩包根目录",
    solid: "固实",
    encrypted: "有密码保护",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "删除此文件夹？" : "删除此镜像？"),
    text: (folder) =>
      `${folder ? "该文件夹及其中所有内容" : "该镜像文件"}将被永久删除：不进废纸篓，无法撤销。如果删除中途停止，部分内容可能已被删除。`,
    no: "否",
    yes: "是，删除",
    archiveTitle: "删除此压缩包？",
    archiveText:
      "该压缩包文件将被永久删除：不进废纸篓，无法撤销。分卷压缩包的其他分卷保持不变。",
  },
  // password.tsx
  password: {
    title: "此压缩包需要密码",
    text: "请输入密码以打开它。",
    wrong: "密码不正确。",
    label: "密码",
    cancel: "取消",
    unlock: "解锁",
    checking: "正在检查…",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader: "无法连接端口 9021 上的 payload 加载器。请先运行自动加载器（越狱），然后重新打开 Dump Forge。",
      missing:
        "未找到 Dump Forge 的已保存副本（/data/dumpforge/ 和 USB 驱动器中都没有）。请先从电脑加载一次；它会为此图标保存一份副本。",
      timeout: "Dump Forge 已启动，但 30 秒后仍未响应。它可能仍在启动中（主机从待机模式唤醒时较慢）。",
      stopping: "上一个 Dump Forge 仍在停止中（正在取消任务并清理）。请稍候再试。",
    },
    loadFailed: (e) => `无法加载页面：${e}`,
    noAnswer: "Dump Forge 没有响应",
    cantStart: "无法启动 Dump Forge",
    retry: "重试",
    starting: "正在启动 Dump Forge…",
    client: {
      unreachable: "无法连接 Dump Forge。请确认 payload 仍在运行，且此设备与主机在同一网络中。",
      noPlatform: "无法确定 Dump Forge 是否运行在 PS5 上。请刷新页面重试。",
      quitUnreachable: "无法连接 Dump Forge 以将其停止。",
      stillAnswering: "2 分钟后 Dump Forge 仍在响应；请检查 PS5。",
      gone: "Dump Forge 已不再报告此任务（在看到结果之前，它已从任务历史中移除）",
    },
  },
  // target.tsx
  target: {
    format: {
      folder: { text: "将镜像或安装包解压回普通游戏文件夹。" },
      exfat: {
        text: "适用于以 ffpkg 运行异常、只能像外置硬盘内容那样运行的游戏。也可在 Mac 或 PC 上打开。",
      },
      ffpkg: { tone: "good", text: "推荐。PS5 自身的文件系统（UFS2）。可以以可写方式挂载。" },
      ffpfs: {
        tone: "warn",
        text: "实验性。PS5 的 PFS 文件系统，不压缩。文件名只能使用纯 ASCII 字符。",
      },
      ffpfsc: {
        tone: "warn",
        text: "实验性。体积最小：包裹一个镜像的压缩容器，始终以只读方式挂载。",
      },
      pkg: {
        text: "像商店游戏一样安装，以原生速度运行。需要 kstuff、fpkg-enable 和 ppr-patch。",
      },
      lz4: {
        tone: "warn",
        text: "实验性。资源被打包为 LZ4 卷的游戏文件夹，由内置的 AMPR 模拟器（ampr_emu）在游戏运行时解包。仅适用于使用 AMPR 的游戏。",
      },
    },
    innerFfpkg: { tone: "good", text: "推荐。PS5 自身的文件系统（UFS2）。" },
    level0: { text: "不压缩：每个块按原样存储。文件与镜像一样大。" },
    levelDefault: {
      tone: "good",
      text: "推荐：zlib 的默认级别。级别 1 的构建速度约为级别 6 的 3 倍，文件约大 3%；级别 9 的文件仅略小。",
    },
    levelLow: {
      text: "构建速度比级别 6 快（级别 1 约为级别 6 的 3 倍）；但文件更大（级别 1 约大 3%）。",
    },
    levelHigh: {
      text: "仅比级别 6 略小（级别 9 小 0.04%），但压缩更久（级别 9 约多 25%），且全程占满所有核心。",
    },
    kraken: {
      fast: { label: "快速", note: { tone: "good", text: "推荐。构建最快。" } },
      balanced: {
        label: "均衡",
        note: {
          text: "比“快速”约小 2.6%，但压缩耗时约为 6 倍，且全程占满所有核心。",
        },
      },
      smallest: {
        label: "最小",
        note: {
          text: "比“快速”约小 2.7%，但压缩耗时约为 9 倍，且全程占满所有核心。",
        },
      },
      stored: {
        label: "仅存储",
        note: {
          text: "不压缩：软件包与游戏大小相当。如果压缩的软件包能安装却无法启动（有 8.60 和 10.40 固件上的报告），请试试这个。",
        },
      },
    },
    pkgFirmware: "仅适用于 11.60 及以下固件。",
    imageInside: "内部镜像",
    imageInsideLabel: "ffpfsc 内部的镜像",
    compressionLevel: "压缩级别",
    compression: "压缩",
    fullVerify: "完整校验",
    fullVerifyHint: "重新读取每个字节来核对输出。耗时更长。",
    separator: "名称中不能包含文件夹分隔符；请用“更改”选择文件夹。",
    badName: "名称不能为空，不能以点开头，不能以点或空格结尾，也不能包含控制字符。",
    tooLong: (bytes, max, folder) =>
      `过长：${folder ? "" : "扩展名之前"}共 ${bytes} 字节，最多 ${max} 字节（UTF-8）。`,
    pkgExt: (ext) => `以 .${ext} 结尾，但 fpkg 保存为 .pkg：驱动程序由扩展名决定。`,
    otherExt: (ext, target) => `以 .${ext} 结尾，但目标格式是 ${target}：驱动程序由扩展名决定。`,
    extractInto: "解压到此文件夹",
    saveInto: "保存到此文件夹",
    output: "输出",
    change: "更改",
    naming: "正在命名…",
    chooseSourceFirst: "请先选择源",
    fileName: "文件名",
    savedAs: (name, dir) => `将以 ${name} 保存在 ${dir}`,
    inDir: (dir) => `位于 ${dir}`,
    generate: "根据内容生成名称",
    generatePattern: "[游戏名称]-[游戏 ID]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} 现已推出`,
    update: "更新",
    later: "稍后",
    updating: "正在更新…",
    updatingNote: "正在下载新版本，应用将自动重启。",
    downloading: "正在浏览器中下载：解压压缩包，用它替换当前这份程序。",
    failed: (v) => `无法更新到 v${v}`,
    timeout: (v) => `v${v} 未在 1 分钟内启动。如果已下载，请停止 Dump Forge 后重新打开以完成更新。`,
    retry: "重试",
    close: "关闭",
  },
  // api.ts: prettyDuration's units
  duration: { s: (s) => `${s}秒`, ms: (m, s) => `${m}分${s}秒`, hm: (h, m) => `${h}小时${m}分钟` },
};

export default zh;
