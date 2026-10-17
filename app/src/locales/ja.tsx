// Japanese. Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Japanese has one plural form (CLDR "other"), so counts take a counter word and no `plural`.
// Terms: トレース (trace/traces/tracing), パック/アンパック, パッチ適用/パッチ解除, ランタイム.

import { dotted, num } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "ゲームと同じサイズ";

const ja: Messages = {
  // App.tsx
  app: {
    address: (url) => `アドレス：${url}。接続方法と QR コードを表示`,
    screens: "画面",
    convert: "変換",
    active: (n) => `、実行中 ${num(n)} 件`,
    edit: "編集",
    library: "ライブラリ",
    about: "情報",
    offline: "オフライン、再試行中…",
    stop: "Dump Forge を停止",
    stillRunning: (n) => (n === 1 ? "ジョブが実行中です" : `ジョブが ${num(n)} 件実行中です`),
    job: (id) => `ジョブ ${id}`,
    quitNote: (web, n) =>
      web
        ? n === 1
          ? "Dump Forge を停止すると、このジョブは中断され、未完成の出力は削除されます。"
          : "Dump Forge を停止すると、これらのジョブは中断され、未完成の出力は削除されます。"
        : n === 1
          ? "終了すると、このジョブは中断され、未完成の出力は削除されます。"
          : "終了すると、これらのジョブは中断され、未完成の出力は削除されます。",
    keepRunning: "実行を続ける",
    stopAndQuit: (n) => (n === 1 ? "ジョブを停止して終了" : "ジョブをすべて停止して終了"),
    stoppingJobs: "ジョブを停止し、後片付けをしています…",
    stoppingForge: "Dump Forge を停止しています…",
    couldntStop: "Dump Forge を停止できませんでした",
    close: "閉じる",
    tryAgain: "再試行",
    stopped: "Dump Forge は停止しました",
    startAgain: (ps5) => (ps5 ? "続けるには、PS5 の Dump Forge タイルをもう一度開いてください。" : "続けるには、同じ ps5-dump-forge serve コマンドをもう一度実行してください。"),
    confirmStop: (web: boolean) => (web ? "Dump Forge を停止しますか？" : "PS5 Dump Forge を終了しますか？"),
    stopNote: (web, ps5) => (!web ? "実行中のジョブはありません。" : ps5 ? "もう一度起動するには、PS5 の Dump Forge タイルを開いてください。" : "もう一度起動するには、同じ ps5-dump-forge serve コマンドを再実行してください。"),
    quit: "終了",
    github: (name) => `GitHub の ${name}`,
    queued: "待機中",
    progress: (percent, left) => `${percent}% 完了${left ? `、残り ${left}` : ""}`,
  },
  // Convert.tsx
  convert: {
    target: "出力形式",
    targetFormat: "出力形式",
    alreadyFolder: "ソースがすでにフォルダーです",
    lz4Backport:
      "このゲームのアセットは LZ4 でパックされており、LZ4 マニフェストにバックポートが含まれています。先に「LZ4」タブでアンパックしてから、ここでバックポートを除外してください。",
    unreadable: "ビルドできません：このソースを読み込めませんでした（「ソース」を参照）。",
    noContentId: "fpkg としてビルドできません：このゲームの param.json には、パッケージに必要な Content ID がありません。",
    blocked: (first, more) =>
      more > 0
        ? `ビルドできません：${first}（ほか ${num(more)} 件、「ソース」を参照）。`
        : `ビルドできません：${first}。`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `このゲームは AMPR を使いますが ampr_emu がありません。変換すると同梱の ampr_emu ${version} をインストールします。`;
      if (a === "trace") return `変換すると、LZ4 のトレース用ランタイムが同梱のリリース版 ampr_emu ${version} に置き換わります。`;
      if (a === "replace")
        return own
          ? `変換すると、このゲームの ampr_emu ${own} が同梱の ampr_emu ${version} に置き換わります。`
          : `変換すると、バージョン不明のこのゲームの ampr_emu が同梱の ampr_emu ${version} に置き換わります。`;
      return own
        ? `このゲームの ampr_emu ${own} は同梱の ${version} と同じか新しいため、変換してもそのまま残ります。`
        : `このゲームにはすでに同梱の ampr_emu ${version} があり、変換してもそのまま残ります。`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "ampr_emu.index がこのゲームのファイルと一致しません" : state === "damaged" ? "ampr_emu.index を読み込めません" : "このゲームには ampr_emu.index がありません"}。変換すると新しく書き込みます${traces ? "（古いトレースは含めません）" : ""}。`,
    amprPkgIndex: (traces) =>
      `パッケージには、収録するファイルから作った新しい ampr_emu.index が入ります${traces ? "（古いトレースは含めません）" : ""}。`,
    removeBackport: "バックポートを除外",
    removeHint: (libs, emulators, other) =>
      emulators.length === 0
        ? other
          ? `fakelib/ からバックポートライブラリ ${num(libs)} 個を除外します。その他の homebrew ライブラリは残ります。`
          : `fakelib/ からバックポートライブラリ ${num(libs)} 個を除外します。`
        : other
          ? `fakelib/ からバックポートライブラリ ${num(libs)} 個を除外します。エミュレーター（${emulators.join("、")}）とその他の homebrew ライブラリは残ります。`
          : `fakelib/ からバックポートライブラリ ${num(libs)} 個を除外します。エミュレーター（${emulators.join("、")}）は残ります。`,
    cantRemove: (why) => `バックポートを除外できません：${why}`,
    compare: "形式を比較",
    build: "ビルド",
    likelyFail: (fmt) => (
      <>
        <b>{fmt}</b> では失敗する可能性が高いです。「ソース」に表示されたファイル名を受け付けません。
      </>
    ),
    collides: "このファイルはすでにジョブが書き込み中です。もう一度ビルドすると失敗します。別の名前を選んでください。",
    already: (fmt) => (
      <>
        このゲームはすでに <b>{fmt}</b> としてビルド中です。もう一度ビルドすると 2 つ目のコピーが作成されます。
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "良い", ok: "一長一短", bad: "欠点" },
    columns: ["本体での速度", "ディスク上のサイズ"],
    rows: {
      pkg: {
        tagline: "ファームウェアが対応していれば最適",
        cells: ["ネイティブ、フルスピード", "ゲームにより 30〜60% 小さい"],
      },
      lz4: {
        tagline: "実験的、AMPR を使うゲーム向け",
        cells: ["読み込みが 30〜40% 速い（上流の数値）", "ゲームの約半分のサイズ（上流の数値）"],
      },
      ffpkg: { tagline: "ShadowMountPlus に推奨", cells: ["イメージとしてフルスピード", FLAT] },
      exfat: { tagline: "互換性重視", cells: ["イメージとしてフルスピード", FLAT] },
      ffpfs: { tagline: "実験的", cells: ["イメージとしてフルスピード", FLAT] },
      ffpfsc: {
        tagline: "最小、実験的",
        cells: ["150〜250 MB/s、カクつくことあり", "圧縮レベルによる、通常 30〜60% 小さい"],
      },
      folder: { tagline: "通常のゲームファイル", cells: ["ドライブのフルスピード", FLAT] },
    },
    features: [
      ["変換", "フォルダー、ffpkg、exfat、ffpfs、ffpfsc、fpkg の間で双方向に変換します。"],
      ["編集", "ゲームのファイルをフォルダーのように変更し、ビルド前にカバー、ファームウェア、バックポート、DLC、ファイル名の問題を確認できます。"],
      ["削除", "確認のうえ、ゲームをディスクから削除します。"],
      ["LZ4", "AMPR ゲームをトレースし、そのアセットをスマート修復付きでパックします（実験的）。"],
      ["ストリーミング", "任意のソースから任意の出力先へ。一時コピーなしで、ffpfsc 内のイメージやアーカイブ内のゲームでも同様です。"],
      ["アーカイブ", "ゲームを含む .zip、.7z、.rar をフォルダーのように閲覧、変換、パックできます。"],
      ["ライブラリ", "このコンピューターまたは本体にあるゲームを、すぐに変換・編集できます。"],
      ["安全", "依頼しない限りゲームは変更されません。ジャンクファイルはスキップし、不正なファイル名は事前に報告します。"],
      ["USB と SD カード", "exFAT または FAT32 でフォーマットしたメモリーやカードに書き込めます。"],
      ["その場で、または再構築", "収まる場合は編集をイメージに直接反映し、収まらない場合は新しいイメージを作り、検証後に置き換えます。"],
    ],
    howToConnect: "接続方法",
    connectText: (url) => (
      <>ローカルネットワーク上のデバイスのブラウザーで {url} を開くか、このコードをスキャンしてください。</>
    ),
    close: "閉じる",
    about: "情報",
    aboutText: "このツールは、PS5 を含むどのプラットフォームでも、ゲームを任意の形式から別の形式へ変換できます。",
    connectAny: "どのデバイスからでも接続する方法",
    useAddress: (url) => <>{url} を開くか、このコードをスキャンしてください。</>,
    compare: "形式を比較",
    compareLabel: "形式の比較",
    format: "形式",
    upstream: (app) =>
      `上流の数値：ampr_emu と Lazy_AMPR の作者による報告値で、${app} が測定したものではありません。ゲームによって異なります。ampr_emu 0.4.2.1 は上流のテストビルドです。既知の問題：一部のゲームでセーブ時にクラッシュします。`,
    bundled: "LZ4 でパックしたフォルダーは drakmor 氏の ampr_emu 0.4.2.1（GPL-3.0、無改変で同梱）で動作します。ソースコード：",
    lazy: "トレースしてからパックする手順は Nazky 氏の Lazy_AMPR に従っています：",
    unrar:
      ".rar アーカイブは同梱の UnRAR ライブラリ（Alexander Roshal 作）で読み込みます（展開のみ、UnRAR ライセンスに基づく）。",
    language: "言語",
    system: "システム",
  },
  // Edit.tsx
  edit: {
    details: "詳細",
    deleting: "ソースを削除しています…",
    delete: "ソースを削除",
    deleted: "削除しました：",
    filter: "パスで絞り込み",
    filterLabel: "ファイルをパスで絞り込み",
    firstOf: (max, n) => `${num(n)} 件中、最初の ${num(max)} 件`,
    fileCount: (n) => `${num(n)} 個のファイル`,
    noMatch: (f) => `「${f}」に一致するファイルはありません。`,
    noListing: "アーカイブのファイル一覧には対応していません。",
    noParam: "sce_sys/param.json がありません。",
    reader: "読み込み方式",
    leftovers: "残っているファイルとフォルダー",
    found: (n) => `${num(n)} 件`,
    leftoversNote: (app) =>
      `未完成の出力で、通常は中断されたジョブの残りです。見覚えのあるものだけを、ジョブが実行されていないときに削除してください。${app} が自動で削除することはありません。`,
    checked: (n) => `確認したフォルダー（${num(n)}）`,
    /** Edit's folds under the game card. */
    facts: "ゲーム情報",
    explorer: "ゲームのファイル",
    howTitle: "使い方",
    /** The empty state's paragraphs. */
    how: (app) => [
      "ゲームの**フォルダー**か**イメージ**を開き、USB メモリー上のフォルダーと同じように変更します。**ファイルの追加と置き換え**（ドラッグするか「追加」を使用）、**名前の変更、削除**、フォルダーの作成ができます。",
      `**「適用」を押すまで**何も変更されません。変更はここに積み重なり、${app} がそれらをまとめてイメージやフォルダーに直接書き込み、結果を確認します。**マウントされたドライブではありません**。${app} が変更を記録し、自分で適用します。`,
      `変更が収まる場合は**イメージに直接書き込みます**。収まらない場合（内部に空きがない、または形式がその変更を受け付けない場合）は、${app} が変更を加えた新しいイメージを隣に作成し、問題がなければ元のイメージと入れ替えます。どちらの場合も、途中で失敗すればイメージは元のままです。`,
      "アーカイブは閲覧できますが、編集はできません。",
    ],
    location: "ゲーム内の場所",
    root: "ゲーム",
    up: "上へ",
    upTip: "1 つ上のフォルダーへ",
    name: "名前",
    size: "サイズ",
    items: (n) => `${num(n)} 項目`,
    inFolder: (dirs, files) => `フォルダー ${num(dirs)} 個、ファイル ${num(files)} 個`,
    matches: (n) => `${num(n)} 件一致`,
    emptyFolder: "このフォルダーは空です。",
    open: (name) => `${name} を開く`,
    archiveNote: "アーカイブは閲覧できますが、編集はできません。変更するには、先にフォルダーかイメージに変換してください。",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "ファイルを追加",
    addFolder: "フォルダーを追加",
    pickAddFiles: "追加するファイルを選択",
    pickAddFolder: "追加するフォルダーを選択",
    nameLabel: "名前",
    renameItem: (name) => `${name} の名前を変更`,
    removeItem: (name) => `${name} を削除`,
    renameTip: "名前を変更（F2）",
    removeTip: "削除（Delete キー）",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "展開",
    extractItem: (name: string) => `${name} を展開`,
    extractTip: "選択したフォルダーにコピー",
    extractFolder: "フォルダーを展開",
    extractFolderTip: "表示中のフォルダーを選択したフォルダーにコピー",
    extractGameTip: "ゲーム全体を選択したフォルダーにコピー",
    pickExtract: "展開先を選択",
    extractPending: "保留中の変更は含めず、現在の状態のファイルを展開します",
    nameEmpty: "名前を入力してください。",
    nameBad: "名前に / や \\ は使えません。また、. や .. にはできません。",
    nameTaken: (name) => `${name} はここにすでにあります。`,
    markNew: "新規",
    markReplaced: "置き換え",
    markRenamed: "名前変更",
    wasAt: (path) => `元の場所：${path}`,
    sizeLater: "一覧にしきれないほど多くあります。残りは変更の適用時に読み込みます",
    dropInto: (folder) => `ドロップして ${folder} に追加`,
    pending: (n) => `未適用の変更が ${num(n)} 件`,
    undo: "元に戻す",
    undoTip: "最後の変更を元に戻す",
    discard: "すべて破棄",
    discardTip: "未適用の変更をすべて破棄",
    discardTitle: (n) => (n === 1 ? `${num(n)} 件の変更を破棄しますか？` : `${num(n)} 件の変更をすべて破棄しますか？`),
    discardText: "変更は破棄され、ゲームは何も変わりません。この操作は元に戻せません。",
    discardNo: "いいえ",
    changesList: "変更（順番どおり）",
    willNotApply: (n) => `${num(n)} 件の変更はこのままでは適用できません。元に戻すか、すべて破棄してください。`,
    opAdd: (to) => <>{to} を追加</>,
    opAddFolder: (to) => <>フォルダー {to} を追加</>,
    opDelete: (path) => <>{path} を削除</>,
    opRename: (from, to) => <>{from} を {to} に名前変更</>,
    opMkdir: (path) => <>新しいフォルダー {path}</>,
    opFrom: (src) => <>追加元：{src}</>,
    whyInvalid: (path) => <>{path} はゲームのパスとして正しくありません。</>,
    whyMissing: (path) => <>{path} はゲーム内にありません。</>,
    whyExists: (path) => <>{path} はすでにあります。</>,
    whyFile: (path) => <>{path} はファイルです。</>,
    whyFolder: (path) => <>{path} はフォルダーです。</>,
    whyInto: (path) => <>{path} をそれ自身の中には移動できません。</>,
    conflictTitle: (n) => `${num(n)} 個の項目がここにすでにあります`,
    conflictIn: (folder) => <>{folder} 内：</>,
    conflictText:
      "「置き換え」を選ぶと、新しいものが元の場所に入ります。変更を適用すると元の内容は完全に失われ、問題のあるファイルが原因でゲームが起動しなくなることがあります。フォルダーにフォルダーを追加すると統合され、同じ名前のファイルは置き換えられます。",
    replace: "置き換え",
    skip: "スキップ",
    cancel: "キャンセル",
    /** How the changes go in (the pending bar), from core's check before Apply. */
    modeLabel: "適用方法",
    checking: "変更の適用方法を確認しています",
    checkFailed: "変更を確認できませんでした：",
    checkAgain: "もう一度確認",
    inPlace: "その場で変更",
    rebuild: "再ビルド",
    rebuildNeeded: "再ビルドが必要",
    cantApply: "適用できません",
    changedDirectly: "直接変更",
    folderText: "フォルダーに直接書き込みます。",
    inPlaceText: "イメージに書き込みます。",
    inPlaceNeeds: (need, free) => `イメージに書き込みます。空き ${free} のうち ${need} を使います。`,
    noRoom: (need, free) => `変更には ${need} 必要ですが、イメージの空きは ${free} です。`,
    alwaysRebuilt: "ffpfsc と fpkg のイメージは常に再ビルドされます。",
    rebuildText: "新しいイメージを隣に書き込み、検証後に元のイメージと置き換えます。",
    rebuildNeeds: (need, free) =>
      `新しいイメージを隣に書き込み（約 ${need} の空きが必要、空き ${free}）、検証後に元のイメージと置き換えます。`,
    uploadLater: (n) => `このブラウザーからの ${num(n)} 個のファイルは、「適用」を押してアップロードされた後に確認されます。`,
    decidedOnApply: (n) =>
      n === 1
        ? "ほかの変更がこのブラウザーからのファイルに依存しているため、適用方法は「適用」を押してそのファイルがアップロードされた後に決まります。"
        : `ほかの変更がこのブラウザーからの ${num(n)} 個のファイルに依存しているため、適用方法は「適用」を押してそれらがアップロードされた後に決まります。`,
    sourceLocked: "このゲームに変更を適用しています。ジョブが終わってから別のソースを選んでください。",
    notOpened: (path) => <>{path} は開きませんでした。このゲームに変更を適用しています。ジョブが終わってからもう一度開いてください。</>,
    apply: "適用",
    applyTip: "ここにあるすべての変更を適用",
    applyTitle: (n) => `${num(n)} 件の変更を適用しますか？`,
    applyInPlace: (name) => <>変更は {name} に直接書き込まれます。完了後は元に戻せません。</>,
    applyRebuild: (name) => (
      <>変更を加えた新しいイメージを {name} の隣に書き込み、検証してから置き換えます。置き換えた後は元に戻せません。</>
    ),
    closeImage: "ゲームをこのイメージから実行中の場合は、先にゲームを終了してください。",
    closeFolder: "ゲームをこのフォルダーから実行中の場合は、先にゲームを終了してください。",
    applyRestores: "このイメージの前回の編集が中断されています。先にイメージを元の状態に戻します。",
    uploading: (i, n, sent, total) => `ファイル ${num(i)}/${num(n)} をアップロード中：${sent} / ${total}`,
    uploadLabel: "アップロード",
    uploadCancelled: "アップロードをキャンセルしました。ゲームは何も変わっていません。",
    applyRunning: "変更を適用しています。下のジョブが終わるまで編集はできません。",
    applyFailed: "変更は適用されませんでした：",
    applyRefused: "この方法では変更を適用できません：",
    applyCancelled: "ジョブはキャンセルされました。変更はここに残っています。",
    folderPartly: "変更の一部はすでにフォルダーに書き込まれている可能性があります。表示中のファイルが現在の中身です。",
    interrupted: "このイメージの前回の編集が中断されています。次の変更を確認または適用する前に、イメージを元の状態に戻します。",
  },
  // JobList.tsx
  jobs: {
    jobs: "ジョブ",
    pending: (n) => `実行中または待機中 ${num(n)} 件`,
    allFinished: "すべて完了",
    stages: {
      scan: "ソースを読み込み中",
      preflight: "確認中",
      check: "確認中",
      plan: "計画中",
      compress: "圧縮中",
      write: "書き込み中",
      measure: "パックを計測中",
      pack: "パック中",
      verify: "検証中",
      finalize: "仕上げ中",
    },
    steps: {
      scan: "読み込み",
      preflight: "確認",
      check: "確認",
      plan: "計画",
      compress: "圧縮",
      write: "書き込み",
      measure: "計測",
      pack: "パック",
      verify: "検証",
      finalize: "仕上げ",
    },
    stepsLabel: "ステップ",
    stepDone: (s) => `${s}（完了）`,
    stepLater: (s) => `${s}（未開始）`,
    packingWriting: "パックと書き込み中",
    rate: { read: "読み込み", compress: "圧縮", pack: "パック", write: "書き込み" },
    showPath: "パスを表示",
    showFinder: "Finder で表示",
    showExplorer: "エクスプローラーで表示",
    showFolder: "フォルダーで表示",
    lz4: {
      pack: "LZ4 パック",
      unpack: "LZ4 アンパック",
      trace: "LZ4 トレース",
      unpatch: "LZ4 パッチ解除",
      runtime: "AMPR ランタイム",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "編集",
    editRebuild: "編集 · 再ビルド",
    extract: "展開",
    job: (id) => `ジョブ ${id}`,
    done: "完了",
    cancelled: "キャンセル済み",
    failed: "失敗",
    cancelling: "キャンセル中…",
    queued: "待機中",
    dismiss: "閉じる",
    stopJob: "停止しますか？",
    cancel: "キャンセル",
    stopHint: "停止するには、4 秒以内にもう一度「停止しますか？」をクリックしてください。",
    from: "ソース：",
    left: (d) => `残り ${d}`,
    inStep: (time) => `このステップで ${time} 経過`,
    result: (bytes, n) => `${bytes}、${num(n)} 個のファイル →`,
    outputPath: "出力先のパス",
    checks: "チェック",
    verifiedPlain: (n) => `検証済み：${num(n)} 項目のチェックに合格`,
    verifiedFull: (n) => `完全検証に合格：${num(n)} 項目のチェック`,
    verifiedFast: (n) => `高速検証に合格：${num(n)} 項目のチェック`,
    coverage: (checked, total, n) => `${total} のうち ${checked} を ${num(n)} 個のサンプルで確認`,
    cantBuild: "このゲームをビルドできません：",
    cantEdit: "変更を適用できません：",
    cantExtract: "展開できません：",
    cantBuildAs: (f) => (
      <>
        このゲームを <b>{f}</b> としてビルドできません：
      </>
    ),
    log: (n) => `ログ（${num(n)}）`,
    logLabel: "ログ",
  },
  // Library.tsx
  library: {
    pickTitle: "ライブラリのフォルダーを選択",
    scanningIn: (name) => `${name} をスキャン中`,
    scanning: "スキャン中",
    cantScanFolder: "このフォルダーをスキャンできません",
    chooseAnother: "別のフォルダーを選択",
    pickPrompt: "スキャンするライブラリのフォルダーを選んでください",
    choose: "フォルダーを選択",
    cantScan: "ライブラリをスキャンできません：",
    cantDelete: "削除できません：",
    deleted: (path) => `${path} を削除しました`,
    truncated: "スキャンの上限に達しました。この一覧は不完全な可能性があります。",
    limits: "検索はフォルダー 6 階層まで、スキャン開始から約 15 秒で終了します。",
    noRoots: "スキャンできるフォルダーがありません。",
    none: "ゲームが見つかりませんでした。検索は 6 階層までで、システムフォルダーはスキップします。その他のソースは「編集」や「変換」で選択できます。",
    skipped: "スキップした場所とファイル",
    stays: "結果は「更新」を押すまでここに残ります。新しい出力は更新後に表示されます。",
    library: "ライブラリ",
    storageTip: "本体の内蔵ストレージ（/data）の空き容量（前回のスキャン時点）",
    free: (free, total) => (
      <>
        空き <b>{free}</b> / {total}
      </>
    ),
    refresh: "更新",
    cards: "カード表示",
    list: "リスト表示",
    games: "ゲーム",
    edit: (n) => `${n} を編集`,
    convert: (n) => `${n} を変換`,
    remove: (n) => `${n} を削除`,
    lz4Tip: "LZ4 パック済み：ampr_emu のアセットパック",
    lz4Packed: " パック済み",
  },
  // Lz4.tsx
  lz4: {
    title: "LZ4 アセットパック",
    help: "LZ4 の仕組み",
    scenario: "LZ4 のシナリオ",
    scenarios: { trace: "トレース", pack: "パック/アンパック" },
    actions: { patch: "パッチ適用", unpatch: "パッチ解除", pack: "パック", unpack: "アンパック" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "プレイ中にゲームが読み込むアセットを記録し、パックで使えるようにします。"
      : packed
        ? "ゲームの LZ4 パックを通常のファイルに戻します。"
        : "ゲームのアセットを、実行中に ampr_emu が読み込む LZ4 ボリュームにパックします。",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "トレース用ランタイムでこのイメージを再ビルドし、置き換えます。"
        : "このフォルダーにトレース用ランタイムをインストールします。"
      : image
        ? "リリース版ランタイムでこのイメージを再ビルドし、置き換えます。"
        : "このフォルダーにリリース版ランタイムを戻します。",
    noAmpr: "このゲームは AMPR を使用していません",
    noEboot: "eboot.bin がありません",
    packedFirst: "ソースは LZ4 パック済みです。先にアンパックしてください",
    damaged: "LZ4 パックが破損しています",
    archive: "アーカイブはトレースできません。先に変換してください",
    chooseFirst: "先にソースを選んでください",
    notRead: "ソースをまだ読み込んでいません",
    pickProfile: "LZ4 プロファイル",
    profileFilter: "LZ4 プロファイル（TOML）",
    pickTracesFolder: "LZ4 トレース（ampr_commands.bin と ampr_emu.index を含むフォルダー）",
    pickTracesEither: "LZ4 トレース：zip、または両方のファイルを含むフォルダー",
    pickTracesZip: "LZ4 トレース（zip）",
    unreadable: "開始できません：このソースを読み込めませんでした（「ソース」を参照）。",
    pending: (a) => a === "patch"
      ? "ジョブの実行中または待機中はパッチを適用できません。"
      : "ジョブの実行中または待機中はパッチを解除できません。",
    replacing: "このイメージはすでにジョブが再ビルド中です。完了するまでお待ちください。",
    blocked: (a, first, more) => {
      const what = {
        patch: "パッチを適用できません",
        unpatch: "パッチを解除できません",
        pack: "パックできません",
        unpack: "アンパックできません",
      }[a];
      return more > 0 ? `${what}：${first}（ほか ${num(more)} 件、「ソース」を参照）。` : `${what}：${first}。`;
    },
    chooseGame: "先にゲームのフォルダーまたはイメージを選んでください。",
    noAmprGame: "このゲームは AMPR を使用していないため、LZ4 は使えません。",
    additionalContent: "これは追加コンテンツ（eboot.bin なし）のため、LZ4 は使えません。",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `このゲームには ampr_emu がありません。パックすると同梱の ampr_emu ${version} がインストールされます。`
        : a === "patch"
          ? `このゲームには ampr_emu がありません。パッチ適用で同梱のトレース用ランタイム（ampr_emu ${version}）がインストールされます。`
          : `このゲームには ampr_emu がありません。パッチ解除で同梱のリリース版ランタイム（ampr_emu ${version}）がインストールされます。`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "ampr_emu.index がこのゲームのファイルと一致しません" : state === "damaged" ? "ampr_emu.index を読み込めません" : "このゲームには ampr_emu.index がありません"}。${a === "pack" ? "パックすると" : a === "patch" ? "パッチ適用で" : "パッチ解除で"}新しく書き込みます。`,
    unavailable: (items) => `このソースでは使用できません：${items.join("、")}。`,
    readOnly: (kind) => <>
      <b>{kind}</b> は本体では読み取り専用のため、トレースできません。先に <b>ffpkg</b> または <b>exfat</b> に変換してください。
    </>,
    action: "操作",
    traceAction: "トレースの操作",
    changes: (a, image) => a === "patch"
      ? image
        ? "このイメージを変更します：トレース用ランタイムが fakelib/libSceAmpr.sprx を置き換えます。バックアップは残りません。"
        : "このフォルダーを変更します：トレース用ランタイムが fakelib/libSceAmpr.sprx を置き換えます。バックアップは残りません。"
      : image
        ? "このイメージを変更します：リリース版ランタイムに戻し、トレースを削除します。バックアップは残りません。"
        : "このフォルダーを変更します：リリース版ランタイムに戻し、トレースを削除します。バックアップは残りません。",
    replay: (web) => web
      ? "プレイしてからこのソースをもう一度選ぶと、トレースを使えます。「トレース」では zip としても入手できます。"
      : "プレイしてからこのソースをもう一度選ぶと、トレースを使えます。",
    room: "トレース用の領域",
    roomHint: "ゲームのトレース用の空き容量です。image_rw= でマウントしないと何も記録されません。",
    alreadyRelease: "すでにリリース版ランタイムです。インデックスのみ書き直します。",
    spaceHint: "イメージとほぼ同じサイズの空き容量が必要です。イメージはコピーの検証後に置き換えます。",
    deletesTraces: (a, bytes, web) => a === "patch"
      ? web
        ? `パッチを適用すると、このソースのトレース（${bytes}）が削除されます。先にダウンロードしてください。`
        : `パッチを適用すると、このソースのトレース（${bytes}）が削除されます。先にコピーしてください。`
      : web
        ? `パッチを解除すると、このソースのトレース（${bytes}）が削除されます。先にダウンロードしてください。`
        : `パッチを解除すると、このソースのトレース（${bytes}）が削除されます。先にコピーしてください。`,
    useProfile: "代わりにルールプロファイルを使う",
    profile: "プロファイル",
    choose: "選択",
    profileHint: "パックのルールを記した TOML ファイルです。トレースの代わりに使います。",
    target: "出力形式",
    packInto: "パック先",
    direct: "イメージに直接パックします。ゲームのファイルは 2 回読み込みます。",
    targetFormat: "出力形式",
    unpackFolder: "パックを通常のファイルに戻した新しいゲームフォルダー。",
    unpackHint: (n) =>
      n === null
        ? "パックされたファイルを通常のファイルに戻します。"
        : `パックされた ${num(n)} 個のファイルを通常のファイルに戻します。`,
    likelyFail: (fmt) => <>
      <b>{fmt}</b> では失敗する可能性が高いです。「ソース」に表示されたファイル名を受け付けません。
    </>,
    collides: (a, folder) => a === "pack"
      ? folder
        ? "このフォルダーはすでにジョブが書き込み中です。パックするには別の名前が必要です。"
        : "このファイルはすでにジョブが書き込み中です。パックするには別の名前が必要です。"
      : folder
        ? "このフォルダーはすでにジョブが書き込み中です。アンパックするには別の名前が必要です。"
        : "このファイルはすでにジョブが書き込み中です。アンパックするには別の名前が必要です。",
    already: (a, fmt) => a === "pack"
      ? <>
        このゲームはすでに <b>{fmt}</b> として書き込み中です。もう一度パックすると 2 つ目のコピーが作成されます。
      </>
      : <>
        このゲームはすでに <b>{fmt}</b> として書き込み中です。もう一度アンパックすると 2 つ目のコピーが作成されます。
      </>,
    profileRules: "プロファイルのルールに従ってパックします。エンジンが直接開くファイルはそのまま残します。",
    chosenRules: "これらのトレースでゲームが読み込んだファイルをパックします。エンジンが直接開くファイルはそのまま残します。",
    ownRules: "前回トレースしたセッションでゲームが読み込んだファイルをパックします。エンジンが直接開くファイルはそのまま残します。",
    guessRules: "トレースがないため、信頼性の劣る組み込みの推定でパックします。先にトレースするか、トレースかプロファイルを選んでください。",
    recorded: "記録されたトレース",
    recordedHint: (bytes) => `ampr_commands.bin（${bytes}）：パックはこのトレースを使います。`,
    downloadHint: "ゲームを終了して 1 分ほど待ち、コンピューターのブラウザーからダウンロードしてください。その zip はそちらの「パック/アンパック」→「トレース」で選びます。",
    download: (bytes, zip) => `トレースをダウンロード（${bytes}、${zip}）`,
    missingIndex: "このソースには ampr_emu.index がありません。パックには ampr_commands.bin と同じ場所にこれが必要です。",
    truncatedJournal: "末尾が途切れたジャーナルでも、最後の完全なレコードまでパックできます。",
    fakelibWarning: (recorded) => recorded
      ? "トレースが少ない、またはありませんか？fakelib のアップデーターをオフにし、もう一度パッチを適用してプレイし直してください。"
      : "トレース中は fakelib のアップデーターをオフにしてください。オンのままだと何も記録されません。",
    emptyTitle: "AMPR を使うゲームを選んでください",
    experimental: "実験的：元のゲームのコピーを残しておいてください。",
    chooseSource: "ソースを選択",
    source: "ソース",
    traces: "トレース",
    optionalTraces: "トレース（任意）",
    unlock: "ロックを解除して別のトレースを選ぶ",
    loaded: "このソースからトレースを読み込み済みです",
    lockedHint: (bytes) => `このソース自身のトレース（${bytes}）です。別のトレースを選ぶには鍵をクリックしてください。`,
    clear: "クリア",
    zip: "Zip",
    folder: "フォルダー",
    tracesHint: "「トレースをダウンロード」で取得した amprtrace の zip、または両方のトレースファイルを含むフォルダーを選んでください。",
    useOwn: "ソースのトレースを使う",
    patchResult: (a, n) => a === "patch"
      ? `パッチを適用しました：トレース用ランタイムを導入し、インデックスに ${num(n)} 個のファイルを記載しました。`
      : `パッチを解除しました：リリース版ランタイムに戻し、インデックスに ${num(n)} 個のファイルを記載しました。`,
    removed: (files) => `古いトレースファイルを削除しました：${files.join("、")}。`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "LZ4 の仕組み",
    close: "閉じる",
    warning: "実験的、AMPR ゲーム専用です。元のゲームのコピーを残しておいてください。",
    body: () => (
      <>
        <h3>1. パック/アンパック</h3>
        <ol>
          <li>
            元のゲーム、<b>パック/アンパック</b>、トレース（またはプロファイル）を選びます。
          </li>
          <li>パック済みのフォルダーか通常のイメージに書き出します。フォルダーは <b>変換</b> で任意の形式に変換できます。</li>
          <li>
            パック済みのソースでは代わりに <b>アンパック</b> が表示され、ファイルが元に戻ります。
          </li>
          <li>ゲームの ampr_emu が同梱版以降で LZ4 パックを読める場合、パックはそれを残します。それ以外は同梱のリリース版 ampr_emu をインストールします。LZ4 のトレース用ランタイムや、ampr_emu のないゲームでも同じです。</li>
        </ol>

        <h3>2. トレース</h3>
        <ol>
          <li>
            ゲームの <b>ffpkg</b> または <b>exfat</b> を選び、<b>トレース</b> → <b>パッチ適用</b> を実行します。
          </li>
          <li>fakelib のアップデーターをオフにします。オンのままだとトレース用ランタイムが上書きされます。</li>
          <li>
            <span className="path">image_rw=</span> でマウントし、1 セッションプレイしてからゲームを終了します。
          </li>
          <li>
            PS5 のページから <b>トレースをダウンロード</b> するか、ampr_commands.bin と ampr_emu.index をコピーします。
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    drives: "ドライブ",
    location: "場所",
    parent: "親フォルダー",
    allDrives: "すべてのドライブ",
    truncated: (n) => `最初の ${num(n)} 項目のみ表示しています。`,
    loading: "読み込み中",
    noDrives: "ドライブが見つかりません。",
    noFolders: "ここにフォルダーはありません。",
    noImages: "ここにフォルダー、PS5 イメージ、アーカイブはありません。",
    noFiles: (ext) => `ここにフォルダーや ${dotted(ext, " または ")} ファイルはありません。`,
    chooseFiles: (n) => `${num(n)} 個のファイルを選択`,
    nothing: "ここにフォルダーやファイルはありません。",
    cancel: "キャンセル",
    chooseFolder: "このフォルダーを選択",
  },
  // common.tsx
  common: {
    folder: "フォルダー",
    pickFolder: "ゲームフォルダー",
    pickImage: "ゲームイメージまたはアーカイブ",
    pickImageFilter: "イメージとアーカイブ",
    pickFolderButton: "フォルダー",
    pickImageButton: "イメージ/アーカイブ",
    source: "ソース",
    emptyTitle: "ゲームのフォルダー、イメージまたはアーカイブを選んでください",
    emptyText: "PS5 のゲームフォルダー、またはそのイメージ、パッケージ、アーカイブ。",
    accepted: "対応するソース",
    unknownTitle: "不明なタイトル",
    additionalContent: "追加コンテンツ",
    reading: "ソースを読み込み中",
    unreadable: "読み込めないソース",
    backport: "バックポート",
    emulators: "エミュレーター",
    declares: (fw) => `申告値 ${fw}`,
    fwUnknown: "実行ファイルからはファームウェアを判別できません（読み取れる SDK がありません）",
    emulatorList: (names) => `エミュレーター：${names}`,
    otherEmulator: "その他",
    sdk: (v) => `SDK ${v}（申告値）`,
    titleId: "タイトル ID",
    version: "バージョン",
    firmware: "ファームウェア",
    fwDeclared: "申告値",
    fwBackport: "バックポート",
    fwExecutables: "実行ファイル",
    size: "サイズ",
    files: (n) => `${num(n)} 個のファイル`,
    contentId: "コンテンツ ID",
    noAmpr: "eboot.bin は libSceAmpr をインポートしていません",
    runtime: {
      forge_release: "Forge リリース版",
      forge_trace: "Forge トレース用",
      other: "その他",
      none: "なし",
    },
    packed: "パック済み",
    damaged: "パックが破損",
    traced: "トレース済み",
    plain: "通常",
    packedDetail: (files, volumes, percent) =>
      percent === null
        ? `${num(files)} 個のファイル、${num(volumes)} 個のボリューム`
        : `${num(files)} 個のファイル、${num(volumes)} 個のボリューム、サイズの ${num(percent)}%`,
    journal: (size) => `ジャーナル ${size}`,
    noJournal: "ジャーナルはまだありません",
    runtimeIs: (label) => `ランタイム：${label}`,
    index: {
      stale: "ampr_emu.index が古い：変換で更新されます",
      damaged: "ampr_emu.index が破損：変換で書き直されます",
      missing: "ampr_emu.index なし：変換で書き込まれます",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `変換で同梱の ampr_emu ${version} がインストールされます`
        : a === "replace"
          ? `変換でランタイムが同梱の ampr_emu ${version} に置き換わります`
          : `ampr_emu ${version}：変換後もそのまま残ります`,
    backportLibs: (exclusive, n) =>
      exclusive
        ? `fakelib2/（単独でマウント）、バックポートライブラリ ${num(n)} 個`
        : `fakelib/、バックポートライブラリ ${num(n)} 個`,
    findings: "事前チェックの指摘",
    dlcTitle: (n) => `同梱 DLC（${num(n)}）`,
    dlcLabel: "同梱 DLC",
    dlcAll: (note) => `すべての DLC：${note}。`,
    dlcIn: (folder, size) => `${folder}/ 内、${size}`,
    dlcNoData: "DLC エミュレーター（dlc_emu.ini）で解除：権利のみで、独自のデータはありません",
    dlcStatus: (status) =>
      `DLC エミュレーター（dlc_emu.ini）で解除、ステータス：${status || "未設定"}`,
    dlcMerged: "ゲームのフォルダーに統合済み",
    backportFiles: "バックポートのファイル",
    emulatorsIn: "fakelib 内のエミュレーター",
    qr: (url) => `QR コード：${url}`,
    needsPassword: "このアーカイブにはパスワードが必要です。",
    enterPassword: "パスワードを入力",
    inArchive: (ext, size) => `${ext} アーカイブ内 · ${size}`,
    archive: "アーカイブ",
    parts: (n) => `${num(n)} パート`,
    archiveRoot: (root) => `ゲームの場所: ${root}`,
    archiveAtRoot: "ゲームはアーカイブのルートにあります",
    solid: "ソリッド",
    encrypted: "パスワード保護",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "このフォルダーを削除しますか？" : "このイメージを削除しますか？"),
    text: (folder) =>
      folder
        ? "フォルダーとその中身はすべて完全に削除されます。ゴミ箱には入らず、元に戻せません。削除が途中で止まった場合、一部はすでに削除されている可能性があります。"
        : "イメージファイルは完全に削除されます。ゴミ箱には入らず、元に戻せません。削除が途中で止まった場合、一部はすでに削除されている可能性があります。",
    no: "いいえ",
    yes: "はい、削除します",
    archiveTitle: "このアーカイブを削除しますか？",
    archiveText:
      "アーカイブファイルは完全に削除されます。ゴミ箱には入らず、元に戻せません。分割アーカイブのほかのパートはそのまま残ります。",
  },
  // password.tsx
  password: {
    title: "このアーカイブにはパスワードが必要です",
    text: "開くにはパスワードを入力してください。",
    wrong: "パスワードが正しくありません。",
    label: "パスワード",
    cancel: "キャンセル",
    unlock: "ロック解除",
    checking: "確認中…",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "ポート 9021 のペイロードローダーに接続できません。先にオートローダー（脱獄）を実行してから、もう一度 Dump Forge を開いてください。",
      missing:
        "保存された Dump Forge のコピーが見つかりません（/data/dumpforge/ にも USB ドライブにもありません）。一度 PC から読み込んでください。このアイコン用にコピーが保存されます。",
      timeout:
        "Dump Forge は起動しましたが、30 秒経っても応答がありません。まだ起動中の可能性があります（レストモードから復帰した直後の本体は時間がかかります）。",
      stopping: "前の Dump Forge がまだ停止中です（ジョブをキャンセルして後片付けをしています）。少し待ってから、もう一度お試しください。",
    },
    loadFailed: (e) => `ページを読み込めませんでした：${e}`,
    noAnswer: "Dump Forge が応答しません",
    cantStart: "Dump Forge を起動できません",
    retry: "再試行",
    starting: "Dump Forge を起動しています…",
    client: {
      unreachable:
        "Dump Forge に接続できません。ペイロードが実行中で、このデバイスが同じネットワークにあることを確認してください。",
      noPlatform: "Dump Forge が PS5 で動作しているか判別できません。再読み込みしてもう一度お試しください。",
      quitUnreachable: "Dump Forge に接続できないため、停止できません。",
      stillAnswering: "2 分経っても Dump Forge が応答し続けています。PS5 を確認してください。",
      gone: "Dump Forge から報告されなくなりました（結果を確認する前にジョブ履歴から削除されました）",
    },
  },
  // target.tsx
  target: {
    format: {
      folder: { text: "イメージやパッケージを通常のゲームフォルダーに戻します。" },
      exfat: {
        text: "ffpkg では正常に動かず、外部ドライブのコンテンツとしてのみ動作するゲーム向け。Mac や PC でも開けます。",
      },
      ffpkg: { tone: "good", text: "推奨。PS5 本来のファイルシステム（UFS2）です。書き込み可能なマウントにも対応します。" },
      ffpfs: {
        tone: "warn",
        text: "実験的。PS5 の PFS ファイルシステム（非圧縮）です。ファイル名は ASCII のみ使用できます。",
      },
      ffpfsc: {
        tone: "warn",
        text: "実験的。最小サイズ：1 つのイメージを包む圧縮コンテナで、常に読み取り専用でマウントされます。",
      },
      pkg: {
        text: "ストアのゲームと同様にインストールされ、ネイティブの速度で動作します。kstuff、fpkg-enable、ppr-patch が必要です。",
      },
      lz4: {
        tone: "warn",
        text: "実験的。アセットを LZ4 ボリュームにパックしたゲームフォルダーで、ゲームの実行中に同梱の AMPR エミュレーター（ampr_emu）が展開します。AMPR を使うゲーム専用です。",
      },
    },
    innerFfpkg: { tone: "good", text: "推奨。PS5 本来のファイルシステム（UFS2）です。" },
    level0: { text: "圧縮なし：すべてのブロックをそのまま格納します。ファイルはイメージと同じサイズになります。" },
    levelDefault: {
      tone: "good",
      text: "推奨。zlib の既定値です。1 はビルドが約 3 倍速く、ファイルは約 3% 大きくなります。9 はほとんど小さくなりません。",
    },
    levelLow: {
      text: "6 よりビルドが速くなります（レベル 1 で約 3 倍）が、ファイルは大きくなります（レベル 1 で約 3%）。",
    },
    levelHigh: {
      text: "6 よりわずかに小さくなります（レベル 9 で 0.04%）が、圧縮にかかる時間は長くなり（レベル 9 で約 25% 増）、その間すべてのコアが使用されます。",
    },
    kraken: {
      fast: { label: "高速", note: { tone: "good", text: "推奨。最も速くビルドできます。" } },
      balanced: {
        label: "バランス",
        note: {
          text: "「高速」より約 2.6% 小さくなりますが、圧縮に約 6 倍の時間がかかり、その間すべてのコアが使用されます。",
        },
      },
      smallest: {
        label: "最小",
        note: {
          text: "「高速」より約 2.7% 小さくなりますが、圧縮に約 9 倍の時間がかかり、その間すべてのコアが使用されます。",
        },
      },
      stored: {
        label: "無圧縮",
        note: {
          text: "パッケージはゲームとほぼ同じサイズになります。圧縮したパッケージがインストールできても起動しない場合に試してください（ファームウェア 8.60 と 10.40 で報告あり）。",
        },
      },
    },
    pkgFirmware: "ファームウェア 11.60 以下でのみ動作します。",
    imageInside: "内部のイメージ",
    imageInsideLabel: "ffpfsc 内部のイメージ",
    compressionLevel: "圧縮レベル",
    compression: "圧縮",
    fullVerify: "完全検証",
    fullVerifyHint: "出力の全バイトを読み直して検証します。時間がかかります。",
    separator: "名前にフォルダーの区切り文字は使えません。フォルダーは「変更」で指定してください。",
    badName: "名前は空にできず、ドットで始めることも、ドットや空白で終えることも、制御文字を含めることもできません。",
    tooLong: (bytes, max, folder) =>
      folder
        ? `長すぎます：${bytes} バイト（上限 ${max} バイト、UTF-8）。`
        : `長すぎます：拡張子を除いて ${bytes} バイト（上限 ${max} バイト、UTF-8）。`,
    pkgExt: (ext) => `末尾が .${ext} ですが、fpkg は .pkg として保存されます。ドライバーは拡張子で決まります。`,
    otherExt: (ext, target) =>
      `末尾が .${ext} ですが、出力形式は ${target} です。ドライバーは拡張子で決まります。`,
    extractInto: "このフォルダーに展開",
    saveInto: "このフォルダーに保存",
    output: "出力",
    change: "変更",
    naming: "名前を生成中…",
    chooseSourceFirst: "先にソースを選んでください",
    fileName: "ファイル名",
    savedAs: (name, dir) => `${dir} に ${name} として保存`,
    inDir: (dir) => `保存先：${dir}`,
    generate: "内容から名前を生成",
    generatePattern: "[ゲーム名]-[タイトル ID]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} が利用可能です`,
    update: "アップデート",
    later: "後で",
    updating: "アップデート中…",
    updatingNote: "新しいバージョンをダウンロードしています。アプリは自動で再起動します。",
    downloading: "ブラウザーでダウンロードしています。アーカイブを展開して、このコピーと置き換えてください。",
    failed: (v) => `v${v} にアップデートできませんでした`,
    timeout: (v) => `v${v} が 1 分以内に起動しませんでした。ダウンロード済みの場合は、Dump Forge を停止してから開き直すと更新が完了します。`,
    retry: "再試行",
    close: "閉じる",
  },
  duration: { s: (s) => `${s}秒`, ms: (m, s) => `${m}分${s}秒`, hm: (h, m) => `${h}時間${m}分` },
};

export default ja;
