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
    screens: "画面",
    convert: "変換",
    active: (n) => `、実行中 ${num(n)} 件`,
    edit: "編集",
    pack: "パック",
    storage: "ストレージ",
    connect: "接続",
    disconnected: "未接続",
    connected: (n: number) => `接続済み（${num(n)} 台）`,
    about: "情報",
    prevTab: "前のタブ (L2)",
    nextTab: "次のタブ (R2)",
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
    quitting: "PS5 Dump Forge を終了しています…",
    couldntStop: "Dump Forge を停止できませんでした",
    close: "閉じる",
    tryAgain: "再試行",
    stopped: "Dump Forge は停止しました",
    startAgain: (ps5) => (ps5 ? "続けるには、PS5 の Dump Forge タイルをもう一度開いてください。" : "続けるには、同じ ps5-dump-forge serve コマンドをもう一度実行してください。"),
    confirmStop: (web: boolean) => (web ? "Dump Forge を停止しますか？" : "PS5 Dump Forge を終了しますか？"),
    stopNote: (web, ps5) => (!web ? "実行中のジョブはありません。" : ps5 ? "もう一度起動するには、PS5 の Dump Forge タイルを開いてください。" : "もう一度起動するには、同じ ps5-dump-forge serve コマンドを再実行してください。"),
    peerBusy: "ペアリング済みのデバイスがこのコンピューターのファイルを読み書きしています。終了するとその転送は止まります。",
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
      "このゲームのアセットは LZ4 でパックされており、LZ4 マニフェストにバックポートが含まれています。先に「パック」タブでアンパックしてから、ここでバックポートを除外してください。",
    zstdBackport:
      "このゲームのアセットは zstd でパックされており、zstd インデックスにバックポートが含まれています。先に「パック」タブでアンパックしてから、ここでバックポートを除外してください。",
    unreadable: "ビルドできません：このソースを読み込めませんでした（「ソース」を参照）。",
    patchBlocked: "ビルドできません：これはパッチパッケージで、アップデートで変更された部分だけを含みます。ファイルを抽出するには「編集」で開いてください。",
    openInEdit: "「編集」で開く",
    noContentId: "fpkg としてビルドできません：このゲームの param.json には、パッケージに必要な Content ID がありません。",
    blocked: (first, more) =>
      more > 0
        ? `ビルドできません：${first}（ほか ${num(more)} 件、「ソース」を参照）。`
        : `ビルドできません：${first}。`,
    amprZstdRuntime: "この変換で fakelib/libSceAmpr.sprx に ampr-emu-ng がインストールされます。これらのパックに必要です。",
    amprNgReplaced: (version) => `このゲームの ampr-emu-ng を ampr_emu ${version} に置き換えます。`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `このゲームは AMPR を使いますが ampr_emu がありません。変換すると同梱の ampr_emu ${version} をインストールします。`;
      if (a === "trace") return `変換すると、このゲームの ampr_emu が同梱の ampr_emu ${version} に置き換わります。`;
      if (a === "replace")
        return own
          ? `変換すると、このゲームの ampr_emu ${own} が同梱の ampr_emu ${version} に置き換わります。`
          : `変換すると、バージョン不明のこのゲームの ampr_emu が同梱の ampr_emu ${version} に置き換わります。`;
      return own
        ? `このゲームの ampr_emu ${own} は同梱の ${version} と同じか新しいため、変換してもそのまま残ります。`
        : `このゲームにはすでに同梱の ampr_emu ${version} があり、変換してもそのまま残ります。`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index がこのゲームのファイルと一致しません" : state === "damaged" ? "ampr_emu.index を読み込めません" : "このゲームには ampr_emu.index がありません"}。変換すると新しく書き込みます。`,
    amprPkgIndex: "パッケージには、収録するファイルから作った新しい ampr_emu.index が入ります。",
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
      zstd: { tagline: "AMPR を使うゲーム向け、ampr-emu-ng で動作", cells: ["読み込みが最大 80% 高速", "ゲームのサイズを最大で半分に"] },
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
      ["転送", "SMB、FTP、NFS 経由で、PS5、コンピューター、NAS の間でファイルを送信し、ゲームを変換します。"],
      ["編集", "ゲームのファイルをフォルダーのように変更し、ビルド前にカバー、ファームウェア、バックポート、DLC、ファイル名の問題を確認できます。"],
      ["ストリーミング", "任意のソースから任意の出力先へ。一時コピーなしで、ffpfsc 内のイメージやアーカイブ内のゲームでも同様です。"],
      ["アーカイブ", "ゲームを含む .zip、.7z、.rar をフォルダーのように閲覧、変換、パックできます。"],
      ["USB と SD カード", "exFAT または FAT32 でフォーマットしたメモリーやカードに書き込めます。"],
      ["パック", "zstd または LZ4（実験的）のアセットパックをスマート修復付きで作成します。"],
      ["ストレージ", "ゲームのライブラリと、名前の変更・移動・削除ができるファイルエクスプローラー。"],
      ["安全", "依頼しない限りゲームは変更されません。ジャンクファイルはスキップし、不正なファイル名は事前に報告します。"],
    ],
    about: "情報",
    compare: "形式を比較",
    compareLabel: "形式の比較",
    format: "形式",
    credits: "クレジット",
    upstream: (app) =>
      `上流の数値：ampr_emu と Lazy_AMPR の作者による報告値で、${app} が測定したものではありません。ゲームによって異なります。ampr_emu 0.4.2.1 は上流のテストビルドです。既知の問題：一部のゲームでセーブ時にクラッシュします。`,
    bundled: "LZ4 でパックしたフォルダーは drakmor 氏の ampr_emu 0.4.2.1（GPL-3.0、無改変で同梱）で動作します。ソースコード：",
    lazy: "LZ4 パックは Nazky 氏の Lazy_AMPR を基にしています：",
    ng: "zstd と LZ4 のパックは ampr-emu-ng で動作します。ソース：",
    logo: "ロゴは Xxcco 氏の作品です：",
    unrar:
      ".rar アーカイブは同梱の UnRAR ライブラリ（Alexander Roshal 作）で読み込みます（展開のみ、UnRAR ライセンスに基づく）。",
    language: "言語",
    system: "システム",
    notifications: "ジョブの完了時に通知する",
    notificationsHint: "変換、パック、転送ジョブの通知を本体に表示します。",
    notificationsFailed: "通知設定を変更できませんでした。",
  },
  // Edit.tsx
  edit: {
    details: "詳細",
    deleting: "ソースを削除しています",
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
      "ゲームの**フォルダー**か**イメージ**を開き、USB メモリー上のフォルダーと同じように変更します。**ファイルの追加と置き換え**（ドラッグするか「追加」を使用）、**名前の変更と削除**ができます。",
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
    actions: "操作",
    items: (n) => `${num(n)} 項目`,
    inFolder: (dirs, files) => `フォルダー ${num(dirs)} 個、ファイル ${num(files)} 個`,
    matches: (n) => `${num(n)} 件一致`,
    emptyFolder: "このフォルダーは空です。",
    open: (name) => `${name} を開く`,
    archiveNote: "アーカイブは閲覧できますが、編集はできません。変更するには、先にフォルダーかイメージに変換してください。",
    nasNote: "NAS 上のゲームは閲覧と抽出はできますが、編集はできません。変更するには、先にこのデバイスへ変換または抽出してください。",
    patchNote: "これはパッチパッケージで、アップデートで変更された部分だけを含みます。完全に含まれているファイルだけを表示しています。閲覧と抽出はできますが、編集はできません。",
    peerNote: "別の Forge 上のゲームは閲覧と抽出はできますが、編集はできません。変更するには、先にこのデバイスへ変換または抽出してください。",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "ファイルを追加",
    addFolder: "フォルダーを追加",
    pickAddFiles: "追加するファイルを選択",
    pickAddFolder: "追加するフォルダーを選択",
    peerRoot: "デバイス全体は追加できません。その中のフォルダーを選んでください。",
    renameItem: (name) => `${name} の名前を変更`,
    removeItem: (name) => `${name} を削除`,
    renameTip: "名前を変更（F2）",
    removeTip: "削除（Delete キー）",
    removeTipMac: "削除（fn+Delete キー）",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extractItem: (name: string) => `${name} を展開`,
    extractTip: "選択したフォルダーにコピー",
    extractFolder: "フォルダーを展開",
    extractFolderTip: "表示中のフォルダーを選択したフォルダーにコピー",
    extractGameTip: "ゲーム全体を選択したフォルダーにコピー",
    extractNotOnDisk: "このフォルダーは保留中の変更にのみ存在します。展開するには変更を適用してください。",
    pickExtract: "展開先を選択",
    extractPending: "保留中の変更は含めず、現在の状態のファイルを展開します。",
    nameEmpty: "名前を入力してください。",
    nameBad: "名前に / や \\ は使えません。また、. や .. にはできません。",
    nameTaken: (name) => `${name} はここにすでにあります。`,
    markNew: "新規",
    markReplaced: "置き換え",
    markRenamed: "名前変更",
    wasAt: (path) => `元の場所：${path}`,
    sizeLater: "一覧にしきれないほど多くあります。残りは変更の適用時に読み込みます",
    dropInto: (folder) => `ドロップして ${folder} に追加`,
    droppedGone: (path) => `${path}: ドロップしたファイルが見つかりません`,
    pending: (n) => `未適用の変更が ${num(n)} 件`,
    undo: "元に戻す",
    undoTip: "最後の操作をその変更ごと元に戻す",
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
    nothingToChange: "変更するものはありません。ゲームはそのままです。",
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
    uploadingFile: (i, n) => `ファイル ${num(i)}/${num(n)} をアップロード中`,
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
      install: "インストール中",
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
      install: "インストール",
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
    packTag: {
      lz4Pack: (level: string) => `LZ4 パック · ${level}`,
      lz4Unpack: "LZ4 アンパック",
      zstdPack: (level: string) => `zstd パック · ${level}`,
      level: (n: number) => `レベル ${num(n)}`,
      zstdUnpack: "zstd アンパック",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "編集",
    editRebuild: "編集 · 再ビルド",
    transfer: "転送",
    extract: "展開",
    job: (id) => `ジョブ ${id}`,
    done: "完了",
    cancelled: "キャンセル済み",
    failed: "失敗",
    cancelling: "キャンセル中…",
    queued: "待機中",
    dismiss: "閉じる",
    retry: "再試行",
    retrying: "開始しています",
    stopJob: "停止しますか？",
    cancel: "キャンセル",
    stopHint: "停止するには、4 秒以内にもう一度「停止しますか？」を押してください。",
    remoteLabel: (name, device) => `${name}（${device} のジョブ）`,
    remoteTitle: (device) => `${device} で実行中で、このデバイスのファイルを使っています。ここではキャンセルだけができます`,
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
    cantTransfer: "転送できません：",
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
    library: "ライブラリ",
    storageTip: "本体の内蔵ストレージ（/data）の空き容量",
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
    deleting: (n) => `${n} を削除しています`,
    lz4Tip: "LZ4 アセットパック",
    zstdTip: "zstd アセットパック",
    showAs: "表示形式",
    kinds: "種類で表示",
    all: "すべて",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n, path) => (
      <>
        <b>{num(n)}</b> 本のゲーム、場所： {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "ストレージの表示",
    library: "ライブラリ",
    files: "ファイルエクスプローラー",
    title: "ファイルエクスプローラー",
    pickTitle: "参照するフォルダーを選択",
    pickPrompt: "ファイルを参照するフォルダーを選んでください",
    choose: "フォルダーを選択",
    roots: "ドライブ",
    location: "場所",
    folders: "フォルダー",
    up: "上へ",
    upTip: "1 つ上のフォルダーへ",
    refresh: "更新",
    filter: "名前で絞り込み",
    filterLabel: "このフォルダーを名前で絞り込み",
    inFolder: (dirs, files) => `フォルダー ${num(dirs)} 個、ファイル ${num(files)} 個`,
    matches: (n) => `${num(n)} 件一致`,
    firstOf: (max, n) => `${num(n)} 件中、最初の ${num(max)} 件`,
    truncated: "このフォルダーには 10,000 を超える項目があります。最初の 10,000 件だけを表示しています。",
    name: "名前",
    size: "サイズ",
    emptyFolder: "このフォルダーは空です。",
    noMatch: (f) => `「${f}」に一致する項目はありません。`,
    noRoots: "参照できるドライブがありません。",
    cantList: "このフォルダーを開けません：",
    link: "リンク",
    linkTip: "リンク：開くことはできますが、ここで名前の変更、移動、削除はできません",
    mounted: "マウント中",
    mountedTip: "ShadowMount+ がマウント中です。削除、名前の変更、移動をするには、ゲームを終了して ShadowMount+ をオフにしてください",
    mountedNote: "「マウント中」と表示されたイメージは ShadowMount+ が使用中です。削除、名前の変更、移動をするには、ゲームを終了して ShadowMount+ をオフにしてください。",
    open: (n) => `${n} を開く`,
    more: (n) => `${n} のその他の操作`,
    rename: (n) => `${n} の名前を変更`,
    renameTip: "名前を変更（F2）",
    move: (n) => `${n} を移動`,
    moveTip: "別のフォルダーへ移動",
    moveTitle: (n) => `${n} の移動先`,
    remove: (n) => `${n} を削除`,
    removeTip: "削除（Delete キー）",
    removeTipMac: "削除（fn+Delete キー）",
    cantDelete: "削除できません：",
    cantRename: "名前を変更できません：",
    cantMove: "移動できません：",
    sameFolder: "すでにそのフォルダーにあります。",
    deleted: (path) => `${path} を削除しました`,
    renamed: (path) => `${path} に名前を変更しました`,
    moved: (path) => `${path} に移動しました`,
    devices: "デバイス",
    thisDevice: "このデバイス",
    otherDevice: "「移動」は 1 台のデバイス内だけで使えます。別のデバイスへコピーするには、そのデバイスのタブへドラッグしてください。",
    dropCopy: (folder) => `ドロップして ${folder} にコピー`,
    dropMove: (folder) => `ドロップして ${folder} に移動`,
    cantCopy: "コピーできません：",
    copyNoFolder: "先にそのデバイスでフォルダーを開いてください。",
    copyOne: (name, to) => `${name} を ${to} にコピーしています。転送は「接続」に表示されます。`,
    copyMany: (n, to) => `${num(n)} 項目を ${to} にコピーしています。転送は「接続」に表示されます。`,
    openConnect: "「接続」を開く",
  },
  // Pack.tsx
  pack: {
    title: "パック",
    help: "パックの仕組み",
    engine: "パックの種類",
    engines: { zstd: "zstd", lz4: "LZ4" },
    actions: { pack: "パック", unpack: "アンパック" },
    info: (e, packed) =>
      e === "zstd"
        ? packed
          ? "zstd パックを通常のファイルに戻します。"
          : "ゲームのアセットを zstd ボリュームにパックします。"
        : packed
          ? "実験的。LZ4 パックを通常のファイルに戻します。"
          : "実験的：一部のゲームは起動しない場合があります。ゲームのアセットを LZ4 ボリュームにパックします。",
    chooseGame: "先にゲームを選んでください。",
    noAmprGame: (e) => `このゲームは AMPR を使用していないため、${e === "zstd" ? "zstd" : "LZ4"} パックは使えません。`,
    patch: "パッチパッケージはパックできません。",
    additionalContent: "追加コンテンツ（eboot.bin なし）はパックできません。",
    unreadable: "開始できません：ソースを読み込めませんでした（「ソース」を参照）。",
    blocked: (a, first, more) =>
      `${a === "pack" ? "パック" : "アンパック"}できません：${first}${more > 0 ? `（ほか ${num(more)} 件、「ソース」を参照）` : ""}。`,
    target: "出力形式",
    packFolder: (e) => `推奨。最速で遅延も最小：アセットを ${e === "zstd" ? "zstd" : "LZ4"} パックにした新しいゲームフォルダー。`,
    levels: { fast: "高速", medium: "標準", max: "最大" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "推奨。zstd 5：速く、サイズも十分。" },
        medium: { text: "zstd 9：より小さく、より遅い。" },
        max: { text: "zstd 19：最小。かなり遅く、メモリも多く使います。" },
      },
      lz4: {
        fast: { tone: "good", text: "推奨。LZ4 の既定：最速。" },
        medium: { text: "LZ4 HC 9：より小さく、より遅い。" },
        max: { text: "LZ4 HC 12：最小、最も遅い。" },
      },
    },
    runtime: "ランタイム",
    runtimeNotes: {
      ng: { tone: "good", text: "推奨。zstd と LZ4 のパックを読み込めます。" },
      ampr_emu: { text: "上流の ampr_emu。" },
    },
    ngNote: "ampr-emu-ng が必要です：パックすると fakelib/ にインストールされます。ShadowMount+ の fakelib 更新をオフにしてください。オンのままだとランタイムが置き換えられます。",
    installsRuntime: (version) => `ampr_emu がありません：パックすると ampr_emu ${version} がインストールされます。`,
    freshIndex: (state) =>
      `${state === "stale" ? "ampr_emu.index が古くなっています" : state === "damaged" ? "ampr_emu.index が破損しています" : "ampr_emu.index がありません"}：パックすると新しく書き込まれます。`,
    direct: "イメージに直接パックします。ゲームは 2 回読み込みます。",
    unpackFolder: "通常のファイルからなる新しいゲームフォルダー。",
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
    emptyTitle: "AMPR を使うゲームを選んでください",
    experimental: "実験的：元のゲームのコピーを残しておいてください。",
    source: "ソース",
  },
  // PackHelp.tsx
  packHelp: {
    title: "パックの仕組み",
    close: "閉じる",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>ゲームのアセットを zstd ボリュームにパックし、フォルダーかイメージに書き込みます。AMPR を使うゲーム専用です。</li>
          <li><b>高速</b>（zstd 5）は速く、<b>標準</b>（9）と<b>最大</b>（19）はより小さく、より遅くなります。</li>
          <li>パック済みのゲームには<b>アンパック</b>が表示され、通常のファイルに戻せます。</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>実験的</b>：一部のゲームは起動しない場合があります。コピーを残しておいてください。</li>
          <li>LZ4 ボリュームにパックし、フォルダーかイメージに書き込みます：<b>高速</b>（LZ4）、<b>標準</b>（HC 9）、<b>最大</b>（HC 12）。</li>
          <li>ランタイム：<b>ampr-emu-ng</b>（既定）または上流の <b>ampr_emu</b>。</li>
          <li>パック済みのゲームには<b>アンパック</b>が表示されます。</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>zstd と LZ4 のパックを読み込みます。パックすると fakelib/ にインストールされます。</li>
          <li>ShadowMount+ の fakelib 更新をオフにしてください。オンのままだとランタイムが置き換えられ、ゲームがパックを読み込めなくなります。</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "デバイス",
    thisDevice: "このデバイス",
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
    chooseFiles: (n) => (n === 0 ? "ファイルを選択" : `${num(n)} 個のファイルを選択`),
    nothing: "ここにフォルダーやファイルはありません。",
    cantList: "このフォルダーを開けません：",
    cancel: "キャンセル",
    chooseFolder: "このフォルダーを選択",
  },
  // common.tsx
  common: {
    folder: "フォルダー",
    pickButton: "選択",
    pickAny: "ゲームのフォルダ、イメージまたはアーカイブ",
    source: "ソース",
    emptyTitle: "ゲームのフォルダー、イメージまたはアーカイブを選んでください",
    accepted: "対応するソース",
    unknownTitle: "不明なタイトル",
    patch: "パッチ",
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
      forge_trace: "旧 LZ4 トレース用ランタイム",
      zstd: "ampr-emu-ng",
      other: "その他",
      none: "なし",
    },
    packed: "パック済み",
    zstdKept: "zstd のアセットパック：変換しても保持されます",
    damaged: "パックが破損",
    plain: "通常",
    packedDetail: (files, volumes, percent) =>
      percent === null
        ? `${num(files)} 個のファイル、${num(volumes)} 個のボリューム`
        : `${num(files)} 個のファイル、${num(volumes)} 個のボリューム、サイズの ${num(percent)}%`,
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
        : "イメージファイルは完全に削除されます。ゴミ箱には入らず、元に戻せません。",
    no: "いいえ",
    yes: "はい、削除します",
    archiveTitle: "このアーカイブを削除しますか？",
    archiveText:
      "アーカイブファイルは完全に削除されます。ゴミ箱には入らず、元に戻せません。分割アーカイブのほかのパートはそのまま残ります。",
    fileTitle: "このファイルを削除しますか？",
    fileText: "ファイルは完全に削除されます。ゴミ箱には入らず、元に戻せません。",
  },
  // replace.tsx
  replace: {
    title: (n) => `${num(n)} 個の項目がそこにすでにあります`,
    inFolder: (folder) => <>{folder} 内：</>,
    more: (n) => `ほか ${num(n)} 個`,
    text: "「置き換え」を選ぶと、新しいものが元の場所に入ります。元のファイルはゴミ箱にも入らずに完全に失われ、元に戻せません。同じ名前のフォルダーの上に置いたフォルダーは統合され、同じ名前のファイルは置き換えられ、それ以外はそのまま残ります。",
    cancel: "キャンセル",
    replace: "置き換え",
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
      uploadIncomplete: "アップロードが完了しませんでした。",
    },
  },
  // target.tsx
  target: {
    format: {
      folder: { text: "イメージやパッケージを通常のゲームフォルダーに戻します。" },
      exfat: {
        text: "ffpkg では正常に動かず、外部ドライブのコンテンツとしてのみ動作するゲーム向け。Mac や PC でも開けます。",
      },
      ffpkg: { text: "PS5 本来のファイルシステム（UFS2）です。書き込み可能なマウントにも対応します。" },
      ffpfs: {
        text: "PS5 の PFS ファイルシステム（非圧縮）です。ファイル名は ASCII のみ使用できます。",
      },
      ffpfsc: {
        text: "最小サイズ：1 つのイメージを包む圧縮コンテナで、常に読み取り専用でマウントされます。",
      },
      pkg: { tone: "good", text: "推奨。ネイティブのゲームと同様に動作します（ファームウェアが対応している場合のみ）。" },
      lz4: {
        tone: "warn",
        text: "実験的。アセットを LZ4 ボリュームにパックしたゲームフォルダーで、ゲームの実行中に同梱の AMPR エミュレーター（ampr_emu）が展開します。AMPR を使うゲーム専用です。",
      },
    },
    innerFfpkg: { text: "PS5 本来のファイルシステム（UFS2）です。" },
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
    updatingNote: "新しいバージョンをダウンロードしています。このページは自動で新しいバージョンに再読み込みされます。",
    downloading: "ブラウザーでダウンロードしています。アーカイブを展開して、このコピーと置き換えてください。",
    failed: (v) => `v${v} にアップデートできませんでした`,
    timeout: (v) => `v${v} が 1 分以内に起動しませんでした。ダウンロード済みの場合は、Dump Forge を停止してから開き直すと更新が完了します。`,
    retry: "再試行",
    close: "閉じる",
  },
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "このデバイス",
    howItWorks: "接続の方法",
    ipAddress: "IP アドレス",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? `最終オンライン：${num(n)} 分前`
        : unit === "h"
          ? `最終オンライン：${num(n)} 時間前`
          : unit === "d"
            ? `最終オンライン：${num(n)} 日前`
            : "最終オンライン：たった今",
    listenError: (e) => `ほかのデバイスからこのデバイスに接続できません: ${e}`,
    unavailable: (e) => `接続を利用できません: ${e}`,
    loading: "読み込み中",
    discover: "自動検出",
    stop: "停止",
    fileTransfer: "ファイル転送",
    devices: "デバイス",
    show: "表示",
    all: "すべて",
    onlineOf: (n, total) => `${num(total)} 台中 ${num(n)} 台がオンライン`,
    sendFilesTo: (name) => `${name} にファイルを送信`,
    details: (name) => `${name} の詳細`,
    userIs: (user) => `ユーザー ${user}`,
    nearby: "近くのデバイス",
    nearbyNone: "新しいデバイスは見つかりませんでした",
    addDevice: "デバイスを追加",
    addressExample: "192.168.1.20 または nas.local",
    addByProtocol: "またはプロトコルで追加:",
    savedEmpty: "保存済みのデバイスはまだありません",
    savedEmptyHint:
      "「自動検出」はこのネットワーク上の Forge とネットワークストレージを見つけます。デバイスをペアリングしてそのデバイスで許可するか、NAS を保存してください。",
    online: "オンライン",
    offline: "オフライン",
    computer: "コンピューター",
    address: "アドレス",
    keyId: "キー ID",
    lastSeen: "最終オンライン",
    status: "状態",
    never: "なし",
    unknown: "不明",
    forget: "登録解除",
    forgetNote: "「登録解除」はこのデバイス上のペアリングだけを削除します。もう一度使うには、再度ペアリングしてください。",
    close: "閉じる",
    cancel: "キャンセル",
    pairTitle: "デバイスをペアリング",
    pair: "ペアリング",
    connectTo: "接続",
    probing: (address) => `${address} を確認しています`,
    pairing: "ペアリング中",
    waitingFor: (address) => `${address} がペアリングを許可するのを待っています`,
    checkNumber: "確認番号",
    checkHint: "もう一方のデバイスに表示される番号と一致している必要があります。",
    denied: "もう一方のデバイスがペアリングを拒否しました。",
    timedOut: "もう一方のデバイスで時間内にペアリングが許可されませんでした。",
    pairBusy: "もう一方のデバイスでは、ほかのペアリング要求が待機中です。1 分後にもう一度お試しください。",
    alreadyPaired: "そのデバイスにはこのデバイスがすでに保存されています。先にそちらで登録解除してから、もう一度ペアリングしてください。",
    tooManyAttempts: "拒否された要求が多すぎます。10 分後にもう一度お試しください。",
    pairPromptTitle: (name) => `${name} とペアリングしますか？`,
    pairPromptBody: (name, platform, ip) =>
      `${name}（${platform}、${ip}）がこのデバイスとのペアリングを求めています。ペアリングしたデバイスは、このデバイスのファイルを読み取り、新しいファイルを追加できます。`,
    pairPromptCheck: (name) => `${name} に同じ確認番号が表示されている場合にのみ許可してください。`,
    allow: "許可",
    deny: "拒否",
    notPaired: "もう一方のデバイスはこのデバイスを認識していません。もう一度ペアリングしてください。",
    searching: "このネットワークを検索中",
    forge: "Forge",
    save: "保存",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `${protocol} NAS を追加`,
    nasEditTitle: (protocol) => `${protocol} NAS を編集`,
    nasName: "名前",
    nasAddressHint: "IP アドレスまたはホスト名。ポートが 445 以外なら :ポート を付けます",
    nasUser: "ユーザー名",
    nasUserHint: "空欄のままにするとゲストとして接続します",
    ftpAddressHint: "IP アドレスまたはホスト名。21 以外のときは :ポート を付けます",
    nfsAddressHint: "IP アドレスまたはホスト名。2049 以外のときは :ポート を付けます",
    ftpUserHint: "空欄のままにすると匿名で接続します",
    ftpNote: "FTP はログインも含めて暗号化されません。",
    nfsNote: "NFS にはログインがありません。Forge は root として接続する (NFSv4.1、だめなら NFSv3) ため、サーバーのエクスポートでこのデバイスを許可しておく必要があります。NFS は暗号化されません。",
    nasPassword: "パスワード",
    nasPasswordKeep: "空欄のままにすると保存済みのパスワードを使います",
    guest: "ゲスト",
    passwordSaved: "保存済み",
    passwordNone: "なし",
    edit: "編集",
    nasForgetNote: "「登録解除」はこの NAS と保存済みのパスワードをこのデバイスから削除します。",
    pickSendFiles: "送るファイル",
    pickSendFolder: "送るフォルダー",
    pickSaveTo: "保存先",
    thisBrowser: "このブラウザー",
    browserHint: "このスマートフォンやコンピューターのファイルをこのデバイス上のフォルダーに入れ、このデバイス上のファイルをこのブラウザーで保存します。",
    uploadTo: "アップロード先のフォルダー",
    uploadPick: "アップロードするファイルを選択",
    uploading: (name, i, n) => `${name} をアップロード中（${num(i)} / ${num(n)}）`,
    uploaded: "アップロードが終わりました",
    uploadSkipped: (names) => `アップロードしませんでした。そこには同じ名前のフォルダーまたはリンクがあります：${names}。`,
    uploadedBefore: (done, n) => `${num(n)} 個のうち ${num(done)} 個のファイルをアップロードしました。これらはこのデバイスに残ります。`,
    cantStart: "転送を開始できません：",
    cantUpload: "アップロードできません：",
    pickDownload: "ダウンロードするファイル",
    transferIntro: "どのデバイスのファイルやフォルダーでも、このデバイス、ペアリング済みのデバイス、NAS のいずれかのフォルダーに送ります。転送はジョブ一覧のジョブとして実行されます。",
    send: "送信",
    sendWhat: "送るもの",
    sendTo: "送信先",
    sendNeedsDevice: "先に別のデバイスを接続してください。Forge をペアリングするか、NAS を追加します。",
    nothingChosen: "まだ何も選んでいません",
    chooseFiles: "ファイルを選択",
    chooseFolder: "フォルダーを選択",
    startTransfer: "開始",
    uploadHere: "このデバイスにアップロード",
    downloadHere: "このデバイスからダウンロード",
    consoleNoFiles: "PS5 のブラウザーではファイルを選べません。スマートフォンかコンピューターでこのページを開いてください。",
    wholeRoot: "デバイス、共有、ドライブの全体は送れません。その中のフォルダーを選んでください。",
    helpPhone: "QR コードをスキャンすると、スマートフォンでこのページが開きます。そこでは「ファイル転送」の「このデバイスにアップロード」と「このデバイスからダウンロード」で、スマートフォンのファイルをやり取りできます。",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "「接続」は、同じネットワーク上の PS5 とコンピューターにある Dump Forge どうしをつなぎます。",
      "ペアリングするには、「自動検出」を実行して「近くのデバイス」で「ペアリング」を選ぶか、「デバイスを追加」にアドレスを入力し、もう一方のデバイスで許可します。両方の画面に同じ確認番号が表示されているときだけ許可してください。",
      "NAS（SMB、FTP、NFS）も同じように追加します。ペアリングしたデバイスと NAS はゲームの読み込み元にも出力先にもなりますが、そこにあるゲームは編集されません。",
      "「ファイル転送」は、これらの間でファイルやフォルダーをコピーします。転送先に同じ名前がある場合は、確認したあとにだけ置き換えます。",
    ],
    helpSecurity: "デバイス間で送信される内容は暗号化されないため、同じネットワークの誰でも読めます。保存した NAS のパスワードは暗号化されずにこのデバイスに保存されます。「接続」は自宅のネットワークでのみ使ってください。",
    installTitle: "PS5 の Dump Forge",
    ps5Address: "PS5 のアドレス",
    install: "PS5 にインストール",
    installHint: "本体の ELF ローダーがネットワーク上で待ち受けている必要があります (ポート 9021)。",
    findingRelease: "GitHub でリリースを探しています",
    installing: (v, host) => `${host} に Dump Forge ${v} をインストールしています`,
    installed: (v, host) => `${host} で Dump Forge ${v} が起動しています。起動したら「デバイスを追加」からペアリングしてください。`,
    noRelease: "GitHub に Dump Forge の PS5 リリースが見つかりません。このコンピューターのインターネット接続を確認して、もう一度お試しください。",
    otherPayload: "その他の ELF ペイロード:",
    sendPayload: "ペイロードを送信",
    pickPayload: "ELF ペイロードを選択",
    sendingPayload: (name, host) => `${name} を ${host} に送信しています`,
    sentPayload: (name, size, host) => `${name} (${size}) を ${host} に送信しました。`,
    helpInstallTitle: "ワンクリックで PS5 に Dump Forge をインストール",
    helpInstallText: "Dump Forge のインストールカードからペイロードを本体に送ります。GitHub から対応するバージョン (または最新版) をダウンロードして起動します。",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "パッケージインストーラー",
    packages: "パッケージ",
    refresh: "更新",
    scanning: "パッケージを探しています",
    none: "この本体にパッケージが見つかりません。",
    noneHint: "本体のストレージか USB ドライブに .pkg ファイルをコピーしてから、「更新」を押してください。",
    truncated: "一覧が不完全な可能性があります",
    limits: "検索が上限のいずれかに達して止まりました。一部のパッケージが表示されていない可能性があります。",
    cantScan: "検索できません：",
    cantInstall: "インストールできません：",
    content: { game: "ゲーム", patch: "パッチ", additional_content: "追加コンテンツ", unknown: "不明" },
    install: "インストール",
    installName: (name) => `${name} をインストール`,
    started: (name) => `${name} をインストールしています。ジョブは下に表示されます。`,
    tag: "インストール",
    title: "このパッケージをインストールしますか？",
    titleOn: (device) => `このパッケージを ${device} にインストールしますか？`,
    file: "ファイル",
    game: "ゲーム",
    titleId: "タイトル ID",
    version: "バージョン",
    size: "サイズ",
    text: "PS5 が Dump Forge からパッケージを取得してインストールします。ジョブが終わるまで Dump Forge を起動したままにしてください。",
    reinstall: "このゲームがすでにインストールされている場合、PS5 はインストール済みのコピーを削除してからこのパッケージをインストールします。",
    cancel: "キャンセル",
    confirm: "インストール",
    installPkg: "pkg をインストール",
    installPkgOn: (device) => `${device} にパッケージをインストール`,
    pickTitle: "インストールするパッケージを選択",
    sent: (device) => `${device} がパッケージをインストールしています。ジョブは下に表示されます。`,
  },
  duration: { s: (s) => `${s}秒`, ms: (m, s) => `${m}分${s}秒`, hm: (h, m) => `${h}時間${m}分` },
};

export default ja;
