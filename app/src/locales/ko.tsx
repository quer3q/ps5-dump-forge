// Korean. Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Korean has one plural form (CLDR "other" only), so counts use num() with a counter word, no plural().
// Trace terms are written as loanwords: 트레이스, 트레이싱.

import { dotted, num } from "../i18n";
import type { Messages } from "./en";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "게임 크기와 같음";

const ko: Messages = {
  // App.tsx
  app: {
    address: (url) => `주소: ${url}. 연결 방법과 QR 코드 보기`,
    screens: "화면",
    convert: "변환",
    active: (n) => `, ${num(n)}개 진행 중`,
    edit: "편집",
    library: "라이브러리",
    about: "정보",
    offline: "오프라인, 다시 시도 중…",
    stop: "Dump Forge 중지",
    stillRunning: (n) => (n === 1 ? "작업이 아직 실행 중입니다" : `작업 ${num(n)}개가 아직 실행 중입니다`),
    job: (id) => `작업 ${id}`,
    quitNote: (web) =>
      web
        ? "Dump Forge를 중지하면 실행 중인 작업도 중지되고 완료되지 않은 출력은 삭제됩니다."
        : "종료하면 실행 중인 작업도 중지되고 완료되지 않은 출력은 삭제됩니다.",
    keepRunning: "계속 실행",
    stopAndQuit: (n) => (n === 1 ? "작업 중지 후 종료" : "모든 작업 중지 후 종료"),
    stoppingJobs: "작업을 중지하고 정리하는 중…",
    stoppingForge: "Dump Forge 중지 중…",
    couldntStop: "Dump Forge를 중지하지 못했습니다",
    close: "닫기",
    tryAgain: "다시 시도",
    stopped: "Dump Forge가 중지되었습니다",
    startAgain: (ps5) => (ps5 ? "계속하려면 PS5에서 Dump Forge 타일을 다시 여세요." : "계속하려면 같은 ps5-dump-forge serve 명령을 다시 실행하세요."),
    confirmStop: (web: boolean) => (web ? "Dump Forge를 중지할까요?" : "PS5 Dump Forge를 종료할까요?"),
    stopNote: (web, ps5) => (!web ? "실행 중인 작업이 없습니다." : ps5 ? "다시 시작하려면 PS5에서 Dump Forge 타일을 여세요." : "다시 시작하려면 같은 ps5-dump-forge serve 명령을 다시 실행하세요."),
    quit: "종료",
    github: (name) => `GitHub의 ${name}`,
    queued: "대기 중",
    progress: (percent, left) => `${percent}% 완료${left ? `, ${left} 남음` : ""}`,
  },
  // Convert.tsx
  convert: {
    target: "대상",
    targetFormat: "대상 형식",
    alreadyFolder: "소스가 이미 폴더임",
    lz4Backport:
      "이 게임의 에셋은 LZ4로 패킹되어 있고 LZ4 매니페스트에 백포트가 포함되어 있습니다. 먼저 ‘LZ4’ 탭에서 언패킹한 다음 여기에서 백포트를 제거하세요.",
    unreadable: "빌드할 수 없습니다: 이 소스를 읽지 못했습니다(‘소스’ 참고).",
    noContentId: "fpkg로 빌드할 수 없습니다: 이 게임의 param.json에 패키지에 필요한 Content ID가 없습니다.",
    blocked: (first, more) =>
      `빌드할 수 없습니다: ${first}${more > 0 ? ` (외 ${num(more)}개, ‘소스’ 참고)` : ""}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `이 게임은 AMPR을 사용하지만 ampr_emu가 없습니다. 변환하면 번들된 ampr_emu ${version} 버전을 설치합니다.`;
      if (a === "trace") return `변환하면 LZ4 트레이스 런타임이 번들된 릴리스 ampr_emu ${version} 버전으로 바뀝니다.`;
      if (a === "replace")
        return own
          ? `변환하면 이 게임의 ampr_emu ${own} 버전이 번들된 ampr_emu ${version} 버전으로 바뀝니다.`
          : `변환하면 버전을 알 수 없는 이 게임의 ampr_emu가 번들된 ampr_emu ${version} 버전으로 바뀝니다.`;
      return own
        ? `이 게임의 ampr_emu ${own} 버전은 번들된 ${version} 버전과 같거나 더 새로우므로 변환해도 그대로 유지됩니다.`
        : `이 게임에는 이미 번들된 ampr_emu ${version} 버전이 있어 변환해도 그대로 유지됩니다.`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "ampr_emu.index가 이 게임의 파일과 맞지 않습니다" : state === "damaged" ? "ampr_emu.index를 읽을 수 없습니다" : "이 게임에는 ampr_emu.index가 없습니다"}. ${traces ? "변환하면 새 인덱스를 쓰고 이전 트레이스는 제외합니다." : "변환하면 새 인덱스를 씁니다."}`,
    amprPkgIndex: (traces) =>
      traces ? `패키지에는 담긴 파일로 만든 새 ampr_emu.index가 들어가고 이전 트레이스는 제외됩니다.` : `패키지에는 담긴 파일로 만든 새 ampr_emu.index가 들어갑니다.`,
    removeBackport: "백포트 제거",
    removeHint: (libs, emulators, other) =>
      emulators.length === 0
        ? other
          ? `fakelib/에서 백포트 라이브러리 ${num(libs)}개를 제외합니다. 다른 홈브루 라이브러리는 유지됩니다.`
          : `fakelib/에서 백포트 라이브러리 ${num(libs)}개를 제외합니다.`
        : other
          ? `fakelib/에서 백포트 라이브러리 ${num(libs)}개를 제외합니다. 에뮬레이터(${emulators.join(", ")})와 다른 홈브루 라이브러리는 유지됩니다.`
          : `fakelib/에서 백포트 라이브러리 ${num(libs)}개를 제외합니다. 에뮬레이터(${emulators.join(", ")})는 유지됩니다.`,
    cantRemove: (why) => `백포트를 제거할 수 없습니다: ${why}`,
    compare: "형식 비교",
    build: "빌드",
    likelyFail: (fmt) => (
      <>
        <b>{fmt}</b> 형식으로는 실패할 가능성이 높습니다. ‘소스’에 나열된 파일 이름을 허용하지 않습니다.
      </>
    ),
    collides: "이 파일을 이미 쓰고 있는 작업이 있어 다시 빌드하면 실패합니다. 다른 이름을 선택하세요.",
    already: (fmt) => (
      <>
        이 게임을 이미 <b>{fmt}</b> 형식으로 빌드하고 있습니다. 다시 빌드하면 사본이 하나 더 만들어집니다.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "좋음", ok: "절충", bad: "단점" },
    columns: ["콘솔 속도", "디스크 사용량"],
    rows: {
      pkg: {
        tagline: "펌웨어가 허용한다면 최선",
        cells: ["네이티브, 최고 속도", "게임에 따라 30–60% 작음"],
      },
      lz4: {
        tagline: "실험적, AMPR을 사용하는 게임용",
        cells: ["로딩 30–40% 빠름(업스트림 수치)", "게임 크기의 약 절반(업스트림 수치)"],
      },
      ffpkg: { tagline: "ShadowMountPlus에 권장", cells: ["이미지 최고 속도", FLAT] },
      exfat: { tagline: "호환성", cells: ["이미지 최고 속도", FLAT] },
      ffpfs: { tagline: "실험적", cells: ["이미지 최고 속도", FLAT] },
      ffpfsc: {
        tagline: "가장 작음, 실험적",
        cells: ["150–250 MB/s, 끊김이 있을 수 있음", "압축 수준에 따라 다름, 보통 30–60% 작음"],
      },
      folder: { tagline: "일반 게임 파일", cells: ["드라이브 최고 속도", FLAT] },
    },
    features: [
      ["변환", "폴더, ffpkg, exfat, ffpfs, ffpfsc, fpkg 사이를 양방향으로 변환합니다."],
      ["편집", "게임 파일을 폴더처럼 수정하고, 빌드하기 전에 게임의 커버, 펌웨어, 백포트, DLC, 파일 이름 문제를 확인합니다."],
      ["삭제", "확인을 거쳐 디스크에서 게임을 삭제합니다."],
      ["LZ4", "AMPR 게임을 트레이스한 다음 에셋을 스마트 복구와 함께 패킹합니다(실험적)."],
      ["스트리밍", "어떤 원본에서 어떤 대상으로든 임시 복사본 없이 처리합니다. ffpfsc 안의 이미지나 아카이브 안의 게임도 마찬가지입니다."],
      ["아카이브", "게임이 든 .zip, .7z, .rar 를 폴더처럼 탐색, 변환, 패킹합니다."],
      ["라이브러리", "이 컴퓨터나 본체에 있는 게임을 바로 변환하고 편집할 수 있습니다."],
      ["안전", "요청하지 않으면 게임을 절대 변경하지 않으며, 불필요한 파일은 건너뛰고 잘못된 파일 이름은 미리 알려 줍니다."],
      ["USB 및 SD 카드", "exFAT 또는 FAT32로 포맷된 USB 메모리와 카드에 기록합니다."],
      ["제자리 또는 재빌드", "수정 사항이 들어가면 이미지에 바로 반영하고, 아니면 새 이미지를 만들어 검증 후 교체합니다."],
    ],
    howToConnect: "연결 방법",
    connectText: (url) => (
      <>로컬 네트워크에 있는 기기의 브라우저에서 {url} 주소를 열거나 이 코드를 스캔하세요.</>
    ),
    close: "닫기",
    about: "정보",
    aboutText: "이 도구는 PS5를 포함한 모든 플랫폼에서 게임을 어떤 형식에서든 다른 형식으로 변환할 수 있습니다.",
    connectAny: "다른 기기에서 연결하는 방법",
    useAddress: (url) => <>{url} 주소를 사용하거나 이 코드를 스캔하세요.</>,
    compare: "형식 비교",
    compareLabel: "형식 비교",
    format: "형식",
    upstream: (app) =>
      `업스트림 수치: ampr_emu와 Lazy_AMPR 개발자가 보고한 값이며 ${app}에서 측정한 값이 아닙니다. 게임에 따라 달라집니다. ampr_emu 0.4.2.1은 업스트림 테스트 빌드이며, 알려진 문제로 일부 게임이 저장 중에 비정상 종료됩니다.`,
    bundled: "LZ4로 패킹된 폴더는 drakmor의 ampr_emu 0.4.2.1(GPL-3.0)로 실행되며, 수정 없이 번들되어 있습니다. 소스 코드:",
    lazy: "트레이스 후 패킹하는 작업 흐름은 Nazky의 Lazy_AMPR를 따릅니다:",
    unrar:
      ".rar 파일은 함께 제공되는 UnRAR 라이브러리(Alexander Roshal 제작)로 읽습니다(압축 해제만, UnRAR 라이선스 적용).",
    language: "언어",
    system: "시스템",
  },
  // Edit.tsx
  edit: {
    details: "세부 정보",
    deleting: "소스를 삭제하는 중…",
    delete: "소스 삭제",
    deleted: "삭제됨:",
    filter: "경로로 필터",
    filterLabel: "경로로 파일 필터",
    firstOf: (max, n) => `${num(n)}개 중 처음 ${num(max)}개`,
    fileCount: (n) => `파일 ${num(n)}개`,
    noMatch: (f) => `‘${f}’에 해당하는 파일이 없습니다.`,
    noListing: "압축 파일의 파일 목록은 지원하지 않습니다.",
    noParam: "sce_sys/param.json이 없습니다.",
    reader: "리더",
    leftovers: "남은 파일과 폴더",
    found: (n) => `${num(n)}개 발견`,
    leftoversNote: (app) =>
      `완료되지 않은 출력으로, 보통 중단된 작업에서 남은 것입니다. 알아볼 수 있는 것만, 실행 중인 작업이 없을 때 삭제하세요. ${app}는 이를 직접 삭제하지 않습니다.`,
    checked: (n) => `확인한 폴더(${num(n)})`,
    /** Edit's folds under the game card. */
    facts: "게임 정보",
    explorer: "게임 파일",
    howTitle: "사용 방법",
    /** The empty state's paragraphs. */
    how: (app) => [
      "**게임 폴더**나 **이미지**를 열고 USB 메모리의 폴더처럼 수정하세요. **파일을 추가하거나 교체하고**(끌어다 놓거나 ‘추가’ 사용), **이름을 바꾸고, 삭제하고**, 폴더를 만들 수 있습니다.",
      `**‘적용’을 누르기 전에는** 아무것도 바뀌지 않습니다. 변경 사항은 여기에 쌓이고, ${app}가 이를 한 번에 이미지나 폴더에 바로 기록한 뒤 결과를 확인합니다. **마운트된 드라이브가 아닙니다**. ${app}가 변경 사항을 기억해 두었다가 직접 적용합니다.`,
      `변경 사항이 들어갈 공간이 있으면 **이미지에 바로 기록합니다**. 그렇지 않으면(안에 남은 공간이 없거나 형식이 그 변경을 받을 수 없으면) ${app}가 변경 사항을 반영한 새 이미지를 옆에 만들고, 확인이 끝나면 원래 이미지와 바꿉니다. 어느 경우든 도중에 실패하면 이미지는 원래대로 남습니다.`,
      "압축 파일은 둘러볼 수 있지만 편집할 수는 없습니다.",
    ],
    location: "게임 내 위치",
    root: "게임",
    up: "위로",
    upTip: "상위 폴더로",
    name: "이름",
    size: "크기",
    items: (n) => `항목 ${num(n)}개`,
    inFolder: (dirs, files) => `폴더 ${num(dirs)}개, 파일 ${num(files)}개`,
    matches: (n) => `${num(n)}개 일치`,
    emptyFolder: "이 폴더는 비어 있습니다.",
    open: (name) => `${name} 열기`,
    archiveNote: "압축 파일은 둘러볼 수 있지만 편집할 수는 없습니다. 수정하려면 먼저 폴더나 이미지로 변환하세요.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "파일 추가",
    addFolder: "폴더 추가",
    pickAddFiles: "추가할 파일 선택",
    pickAddFolder: "추가할 폴더 선택",
    nameLabel: "이름",
    renameItem: (name) => `${name} 이름 바꾸기`,
    removeItem: (name) => `${name} 삭제`,
    renameTip: "이름 바꾸기(F2)",
    removeTip: "삭제(Delete 키)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "추출",
    extractItem: (name: string) => `${name} 추출`,
    extractTip: "선택한 폴더로 복사",
    extractFolder: "폴더 추출",
    extractFolderTip: "표시된 폴더를 선택한 폴더로 복사",
    extractGameTip: "게임 전체를 선택한 폴더로 복사",
    pickExtract: "추출할 위치 선택",
    extractPending: "보류 중인 변경 사항 없이 현재 상태의 파일을 추출합니다",
    nameEmpty: "이름을 입력하세요.",
    nameBad: "이름에는 / 또는 \\를 넣을 수 없으며, . 또는 ..일 수도 없습니다.",
    nameTaken: (name) => `‘${name}’ 항목이 이미 여기에 있습니다.`,
    markNew: "신규",
    markReplaced: "교체됨",
    markRenamed: "이름 변경됨",
    wasAt: (path) => `이전 위치: ${path}`,
    sizeLater: "목록에 다 표시할 수 없을 만큼 많습니다. 나머지는 변경 사항을 적용할 때 읽습니다",
    dropInto: (folder) => `놓으면 ${folder}에 추가`,
    pending: (n) => `아직 적용하지 않은 변경 ${num(n)}개`,
    undo: "실행 취소",
    undoTip: "마지막 변경 실행 취소",
    discard: "모두 버리기",
    discardTip: "아직 적용하지 않은 변경을 모두 버립니다",
    discardTitle: (n) => (n === 1 ? `변경 ${num(n)}개를 버릴까요?` : `변경 ${num(n)}개를 모두 버릴까요?`),
    discardText: "변경 사항은 사라지고 게임은 아무것도 바뀌지 않습니다. 되돌릴 수 없습니다.",
    discardNo: "아니요",
    changesList: "변경 사항(순서대로)",
    willNotApply: (n) => `변경 ${num(n)}개는 지금 상태로는 적용되지 않습니다. 실행 취소하거나 모두 버리세요.`,
    opAdd: (to) => <>추가: {to}</>,
    opAddFolder: (to) => <>폴더 추가: {to}</>,
    opDelete: (path) => <>삭제: {path}</>,
    opRename: (from, to) => <>이름 바꾸기: {from} → {to}</>,
    opMkdir: (path) => <>새 폴더: {path}</>,
    opFrom: (src) => <>원본: {src}</>,
    whyInvalid: (path) => <>{path}: 올바른 게임 경로가 아닙니다.</>,
    whyMissing: (path) => <>{path}: 게임에 없습니다.</>,
    whyExists: (path) => <>{path}: 이미 있습니다.</>,
    whyFile: (path) => <>{path}: 파일입니다.</>,
    whyFolder: (path) => <>{path}: 폴더입니다.</>,
    whyInto: (path) => <>{path}: 자기 자신 안으로 옮길 수 없습니다.</>,
    conflictTitle: (n) => `항목 ${num(n)}개가 이미 여기에 있습니다`,
    conflictIn: (folder) => <>{folder} 안:</>,
    conflictText:
      "‘교체’를 누르면 새 항목이 그 자리를 대신합니다. 변경 사항을 적용하면 기존 내용은 완전히 사라지며, 잘못된 파일 때문에 게임이 부팅되지 않을 수 있습니다. 폴더 위에 추가한 폴더는 병합되며, 이름이 같은 파일은 교체됩니다.",
    replace: "교체",
    skip: "건너뛰기",
    cancel: "취소",
    /** How the changes go in (the pending bar), from core's check before Apply. */
    modeLabel: "적용 방식",
    checking: "변경 사항을 어떻게 적용할 수 있는지 확인하는 중",
    checkFailed: "변경 사항을 확인하지 못했습니다:",
    checkAgain: "다시 확인",
    inPlace: "제자리 수정",
    rebuild: "다시 빌드",
    rebuildNeeded: "다시 빌드 필요",
    cantApply: "적용할 수 없음",
    changedDirectly: "직접 수정",
    folderText: "폴더에 바로 기록합니다.",
    inPlaceText: "이미지에 기록합니다.",
    inPlaceNeeds: (need, free) => `이미지에 기록합니다(필요 ${need}, 여유 공간 ${free}).`,
    noRoom: (need, free) => `변경 사항에 필요한 공간은 ${need}인데, 이미지의 여유 공간은 ${free}입니다.`,
    alwaysRebuilt: "ffpfsc와 fpkg 이미지는 항상 다시 빌드합니다.",
    rebuildText: "새 이미지를 옆에 기록하고, 검증이 끝나면 원래 이미지를 대체합니다.",
    rebuildNeeds: (need, free) =>
      `새 이미지를 옆에 기록하고(여유 공간 약 ${need} 필요, 현재 ${free}), 검증이 끝나면 원래 이미지를 대체합니다.`,
    uploadLater: (n) => `이 브라우저의 파일 ${num(n)}개는 ‘적용’을 누른 뒤 업로드가 끝나면 확인합니다.`,
    decidedOnApply: (n) =>
      n === 1
        ? "다른 변경 사항이 이 브라우저의 파일을 바탕으로 하므로, 적용 방식은 ‘적용’을 누른 뒤 그 파일의 업로드가 끝나면 정해집니다."
        : `다른 변경 사항이 이 브라우저의 파일 ${num(n)}개를 바탕으로 하므로, 적용 방식은 ‘적용’을 누른 뒤 업로드가 끝나면 정해집니다.`,
    sourceLocked: "이 게임에 변경 사항을 적용하는 중입니다. 작업이 끝난 뒤 다른 소스를 선택하세요.",
    notOpened: (path) => <>{path}: 열지 않았습니다. 이 게임에 변경 사항을 적용하는 중입니다. 작업이 끝난 뒤 다시 여세요.</>,
    apply: "적용",
    applyTip: "여기 나열된 모든 변경 사항 적용",
    applyTitle: (n) => `변경 ${num(n)}개를 적용할까요?`,
    applyInPlace: (name) => <>변경 사항이 {name}에 바로 기록됩니다. 완료되면 되돌릴 수 없습니다.</>,
    applyRebuild: (name) => (
      <>변경 사항을 반영한 새 이미지를 {name} 옆에 기록하고 검증한 뒤 그 자리에 놓습니다. 이미지가 대체되면 되돌릴 수 없습니다.</>
    ),
    closeImage: "게임이 이 이미지에서 실행 중이면 먼저 종료하세요.",
    closeFolder: "게임이 이 폴더에서 실행 중이면 먼저 종료하세요.",
    applyRestores: "이 이미지의 이전 편집이 중단되었습니다. 먼저 이미지를 원래 상태로 되돌립니다.",
    uploading: (i, n, sent, total) => `파일 ${num(i)}/${num(n)} 업로드 중: ${sent} / ${total}`,
    uploadLabel: "업로드",
    uploadCancelled: "업로드를 취소했습니다. 게임은 바뀌지 않았습니다.",
    applyRunning: "변경 사항을 적용하는 중입니다. 아래 작업이 끝날 때까지 편집할 수 없습니다.",
    applyFailed: "변경 사항을 적용하지 못했습니다:",
    applyRefused: "이 방식으로는 변경 사항을 적용할 수 없습니다:",
    applyCancelled: "작업이 취소되었습니다. 변경 사항은 여기에 그대로 남아 있습니다.",
    folderPartly: "일부는 이미 폴더에 기록되었을 수 있습니다. 표시된 파일이 현재 폴더의 내용입니다.",
    interrupted: "이 이미지의 이전 편집이 중단되었습니다. 다음 변경 사항을 확인하거나 적용하기 전에 이미지를 원래 상태로 되돌립니다.",
  },
  // JobList.tsx
  jobs: {
    jobs: "작업",
    pending: (n) => `${num(n)}개 실행 중 또는 대기 중`,
    allFinished: "모두 완료",
    stages: {
      scan: "소스 읽는 중",
      preflight: "확인 중",
      check: "확인 중",
      plan: "계획 중",
      compress: "압축 중",
      write: "쓰는 중",
      measure: "팩 크기 계산 중",
      pack: "패킹 중",
      verify: "검증 중",
      finalize: "마무리 중",
    },
    steps: {
      scan: "읽기",
      preflight: "확인",
      check: "확인",
      plan: "계획",
      compress: "압축",
      write: "쓰기",
      measure: "크기 계산",
      pack: "패킹",
      verify: "검증",
      finalize: "마무리",
    },
    stepsLabel: "단계",
    stepDone: (s) => `${s} (완료)`,
    stepLater: (s) => `${s} (시작 전)`,
    packingWriting: "패킹 및 쓰기 중",
    rate: { read: "읽기", compress: "압축", pack: "패킹", write: "쓰기" },
    showPath: "경로 보기",
    showFinder: "Finder에서 보기",
    showExplorer: "탐색기에서 보기",
    showFolder: "폴더에서 보기",
    lz4: {
      pack: "LZ4 패킹",
      unpack: "LZ4 언패킹",
      trace: "LZ4 트레이스",
      unpatch: "LZ4 패치 해제",
      runtime: "AMPR 런타임",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "편집",
    editRebuild: "편집 · 다시 빌드",
    extract: "추출",
    job: (id) => `작업 ${id}`,
    done: "완료",
    cancelled: "취소됨",
    failed: "실패",
    cancelling: "취소 중…",
    queued: "대기 중",
    dismiss: "닫기",
    stopJob: "중지할까요?",
    cancel: "취소",
    stopHint: "4초 안에 ‘중지할까요?’를 다시 누르면 작업이 중지됩니다.",
    from: "소스: ",
    left: (d) => `${d} 남음`,
    inStep: (time) => `이 단계에서 ${time} 경과`,
    result: (bytes, n) => `${bytes}, 파일 ${num(n)}개 →`,
    outputPath: "출력 경로",
    checks: "확인 항목",
    verifiedPlain: (n) => `검증 완료: 확인 항목 ${num(n)}개 통과`,
    verifiedFull: (n) => `전체 검증 통과: 확인 항목 ${num(n)}개`,
    verifiedFast: (n) => `빠른 검증 통과: 확인 항목 ${num(n)}개`,
    coverage: (checked, total, n) => `샘플 ${num(n)}개로 ${total} 중 ${checked} 확인`,
    cantBuild: "이 게임을 빌드할 수 없습니다:",
    cantEdit: "변경 사항을 적용할 수 없습니다:",
    cantExtract: "추출할 수 없습니다:",
    cantBuildAs: (f) => (
      <>
        이 게임을 <b>{f}</b> 형식으로 빌드할 수 없습니다:
      </>
    ),
    log: (n) => `로그(${num(n)})`,
    logLabel: "로그",
  },
  // Library.tsx
  library: {
    pickTitle: "라이브러리 폴더 선택",
    scanningIn: (name) => `${name} 스캔 중`,
    scanning: "스캔 중",
    cantScanFolder: "이 폴더를 스캔할 수 없습니다",
    chooseAnother: "다른 폴더 선택",
    pickPrompt: "스캔할 라이브러리 폴더를 선택하세요",
    choose: "폴더 선택",
    cantScan: "라이브러리를 스캔할 수 없습니다:",
    cantDelete: "삭제할 수 없습니다:",
    deleted: (path) => `삭제됨: ${path}`,
    truncated: "스캔 한도에 도달했습니다. 목록이 완전하지 않을 수 있습니다.",
    limits: "탐색은 폴더 6단계 깊이까지만, 약 15초 동안만 진행됩니다.",
    noRoots: "스캔할 수 있는 폴더가 없습니다.",
    none: "게임을 찾지 못했습니다. 탐색은 최대 6단계 깊이까지 확인하며 시스템 폴더는 건너뜁니다. 다른 소스는 ‘편집’이나 ‘변환’에서 선택할 수 있습니다.",
    skipped: "건너뛴 위치와 파일",
    stays: "결과는 ‘새로 고침’을 누를 때까지 그대로 유지됩니다. 새 출력은 새로 고친 뒤에 나타납니다.",
    library: "라이브러리",
    storageTip: "마지막 스캔 시점의 콘솔 내부 저장소(/data) 여유 공간",
    free: (free, total) => (
      <>
        {total} 중 <b>{free}</b> 사용 가능
      </>
    ),
    refresh: "새로 고침",
    cards: "카드 보기",
    list: "목록 보기",
    games: "게임",
    edit: (n) => `${n} 편집`,
    convert: (n) => `${n} 변환`,
    remove: (n) => `${n} 삭제`,
    lz4Tip: "LZ4 패킹됨: ampr_emu 에셋 팩",
    lz4Packed: " 패킹됨",
  },
  // Lz4.tsx
  lz4: {
    title: "LZ4 에셋 팩",
    help: "LZ4 사용 방법",
    scenario: "LZ4 시나리오",
    scenarios: { trace: "트레이스", pack: "패킹/언패킹" },
    actions: { patch: "패치", unpatch: "패치 해제", pack: "패킹", unpack: "언패킹" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "플레이하는 동안 게임이 읽는 에셋을 기록해 패킹에 사용합니다."
      : packed
        ? "게임의 LZ4 팩을 일반 파일로 되돌립니다."
        : "게임의 에셋을 게임 실행 중에 ampr_emu가 읽는 LZ4 볼륨으로 패킹합니다.",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "트레이스 런타임으로 이 이미지를 다시 빌드한 다음 교체합니다."
        : "이 폴더에 트레이스 런타임을 설치합니다."
      : image
        ? "릴리스 런타임으로 이 이미지를 다시 빌드한 다음 교체합니다."
        : "이 폴더에 릴리스 런타임을 되돌립니다.",
    noAmpr: "게임이 AMPR을 사용하지 않음",
    noEboot: "eboot.bin이 없음",
    packedFirst: "소스가 LZ4로 패킹됨: 먼저 언패킹하세요",
    damaged: "LZ4 팩이 손상됨",
    archive: "압축 파일은 트레이스할 수 없음: 먼저 변환하세요",
    chooseFirst: "먼저 소스를 선택하세요",
    notRead: "소스를 아직 읽지 않음",
    pickProfile: "LZ4 프로필",
    profileFilter: "LZ4 프로필(TOML)",
    pickTracesFolder: "LZ4 트레이스(ampr_commands.bin과 ampr_emu.index가 있는 폴더)",
    pickTracesEither: "LZ4 트레이스: zip 파일 또는 두 파일이 모두 있는 폴더",
    pickTracesZip: "LZ4 트레이스(zip)",
    unreadable: "시작할 수 없습니다: 이 소스를 읽지 못했습니다(‘소스’ 참고).",
    pending: (a) => a === "patch"
      ? "실행 중이거나 대기 중인 작업이 있는 동안에는 패치할 수 없습니다."
      : "실행 중이거나 대기 중인 작업이 있는 동안에는 패치를 해제할 수 없습니다.",
    replacing: "이 이미지를 이미 다시 빌드하고 있는 작업이 있습니다. 끝날 때까지 기다리세요.",
    blocked: (a, first, more) =>
      `${{ patch: "패치할", unpatch: "패치를 해제할", pack: "패킹할", unpack: "언패킹할" }[a]} 수 없습니다: ${first}${more > 0 ? ` (외 ${num(more)}개, ‘소스’ 참고)` : ""}.`,
    chooseGame: "먼저 게임 폴더나 이미지를 선택하세요.",
    noAmprGame: "이 게임은 AMPR을 사용하지 않으므로 여기서는 LZ4를 쓸 수 없습니다.",
    additionalContent: "추가 콘텐츠(eboot.bin 없음)이므로 여기서는 LZ4를 쓸 수 없습니다.",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `이 게임에는 ampr_emu가 없습니다. 패킹하면 번들된 ampr_emu ${version} 버전이 설치됩니다.`
        : a === "patch"
          ? `이 게임에는 ampr_emu가 없습니다. 패치하면 번들된 트레이스 런타임(ampr_emu ${version})이 설치됩니다.`
          : `이 게임에는 ampr_emu가 없습니다. 패치 해제하면 번들된 릴리스 런타임(ampr_emu ${version})이 설치됩니다.`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "ampr_emu.index가 이 게임의 파일과 맞지 않습니다" : state === "damaged" ? "ampr_emu.index를 읽을 수 없습니다" : "이 게임에는 ampr_emu.index가 없습니다"}. ${a === "pack" ? "패킹하면" : a === "patch" ? "패치하면" : "패치 해제하면"} 새 인덱스를 씁니다.`,
    unavailable: (items) => `이 소스에는 사용할 수 없습니다: ${items.join("; ")}.`,
    readOnly: (kind) => <>
      <b>{kind}</b> 형식은 콘솔에서 읽기 전용이라 트레이스할 수 없습니다. 먼저 <b>ffpkg</b>나 <b>exfat</b>으로 변환하세요.
    </>,
    action: "작업",
    traceAction: "트레이스 작업",
    changes: (a, image) => a === "patch"
      ? image
        ? "이 이미지를 변경합니다. 트레이스 런타임이 fakelib/libSceAmpr.sprx를 대체하며 백업은 없습니다."
        : "이 폴더를 변경합니다. 트레이스 런타임이 fakelib/libSceAmpr.sprx를 대체하며 백업은 없습니다."
      : image
        ? "이 이미지를 변경합니다. 릴리스 런타임이 돌아오고 트레이스가 삭제되며 백업은 없습니다."
        : "이 폴더를 변경합니다. 릴리스 런타임이 돌아오고 트레이스가 삭제되며 백업은 없습니다.",
    replay: (web) => web
      ? "플레이한 다음 이 소스를 다시 선택하면 트레이스를 사용할 수 있습니다. ‘트레이스’에서 zip으로도 받을 수 있습니다."
      : "플레이한 다음 이 소스를 다시 선택하면 트레이스를 사용할 수 있습니다.",
    room: "트레이스 공간",
    roomHint: "게임의 트레이스를 기록할 여유 공간입니다. image_rw=로 이미지를 마운트하지 않으면 아무것도 기록되지 않습니다.",
    alreadyRelease: "이미 릴리스 런타임입니다. 인덱스만 다시 씁니다.",
    spaceHint: "이미지 크기 정도의 여유 공간이 필요합니다. 사본 검증이 끝난 뒤에만 이미지를 교체합니다.",
    deletesTraces: (a, bytes, web) => a === "patch"
      ? web
        ? `패치하면 이 소스의 트레이스(${bytes})가 삭제됩니다. 먼저 다운로드하세요.`
        : `패치하면 이 소스의 트레이스(${bytes})가 삭제됩니다. 먼저 복사해 두세요.`
      : web
        ? `패치를 해제하면 이 소스의 트레이스(${bytes})가 삭제됩니다. 먼저 다운로드하세요.`
        : `패치를 해제하면 이 소스의 트레이스(${bytes})가 삭제됩니다. 먼저 복사해 두세요.`,
    useProfile: "대신 규칙 프로필 사용",
    profile: "프로필",
    choose: "선택",
    profileHint: "트레이스 대신 사용하는, 패킹 규칙이 담긴 TOML 파일입니다.",
    target: "대상",
    packInto: "패킹 대상",
    direct: "이미지에 바로 패킹합니다. 게임 파일은 두 번 읽습니다.",
    targetFormat: "대상 형식",
    unpackFolder: "팩을 일반 파일로 되돌린 새 게임 폴더입니다.",
    unpackHint: (n) =>
      n === null
        ? "패킹된 파일을 일반 파일로 되돌립니다."
        : `패킹된 파일 ${num(n)}개를 일반 파일로 되돌립니다.`,
    likelyFail: (fmt) => <>
      <b>{fmt}</b> 형식으로는 실패할 가능성이 높습니다. ‘소스’에 나열된 파일 이름을 허용하지 않습니다.
    </>,
    collides: (a, folder) => a === "pack"
      ? folder
        ? "이 폴더를 이미 쓰고 있는 작업이 있습니다. 패킹하려면 다른 이름이 필요합니다."
        : "이 파일을 이미 쓰고 있는 작업이 있습니다. 패킹하려면 다른 이름이 필요합니다."
      : folder
        ? "이 폴더를 이미 쓰고 있는 작업이 있습니다. 언패킹하려면 다른 이름이 필요합니다."
        : "이 파일을 이미 쓰고 있는 작업이 있습니다. 언패킹하려면 다른 이름이 필요합니다.",
    already: (a, fmt) => a === "pack"
      ? <>이 게임을 이미 <b>{fmt}</b> 형식으로 쓰고 있습니다. 다시 패킹하면 사본이 하나 더 만들어집니다.</>
      : <>이 게임을 이미 <b>{fmt}</b> 형식으로 쓰고 있습니다. 다시 언패킹하면 사본이 하나 더 만들어집니다.</>,
    profileRules: "프로필 규칙대로 패킹합니다. 엔진이 직접 여는 파일은 그대로 둡니다.",
    chosenRules: "이 트레이스에서 게임이 읽은 파일을 패킹합니다. 엔진이 직접 여는 파일은 그대로 둡니다.",
    ownRules: "마지막 트레이스 세션에서 게임이 읽은 파일을 패킹합니다. 엔진이 직접 여는 파일은 그대로 둡니다.",
    guessRules: "트레이스 없음: 덜 정확한 내장 추정으로 패킹합니다. 먼저 트레이스하거나 트레이스 또는 프로필을 선택하세요.",
    recorded: "기록된 트레이스",
    recordedHint: (bytes) => `ampr_commands.bin(${bytes}): ‘패킹’이 이 트레이스를 사용합니다.`,
    downloadHint: "게임을 닫고 1분 정도 기다린 다음 컴퓨터의 브라우저에서 다운로드하세요. 그곳의 ‘패킹/언패킹’ → ‘트레이스’에서 zip 파일을 선택하세요.",
    download: (bytes, zip) => `트레이스 다운로드(${bytes}): ${zip}`,
    missingIndex: "이 소스에 ampr_emu.index가 없습니다. 패킹하려면 ampr_commands.bin 옆에 이 파일이 있어야 합니다.",
    truncatedJournal: "끝부분이 잘린 저널도 마지막으로 온전한 레코드까지 패킹됩니다.",
    fakelibWarning: (recorded) => recorded
      ? "트레이스가 적거나 없나요? fakelib 업데이터를 끄고 다시 ‘패치’한 다음 다시 플레이하세요."
      : "트레이스하는 동안 fakelib 업데이터를 끄세요. 그렇지 않으면 아무것도 기록되지 않습니다.",
    emptyTitle: "AMPR을 사용하는 게임을 선택하세요",
    experimental: "실험적 기능: 원본 게임의 사본을 보관하세요.",
    chooseSource: "소스 선택",
    source: "소스",
    traces: "트레이스",
    optionalTraces: "트레이스(선택 사항)",
    unlock: "잠금을 해제하고 다른 트레이스 선택",
    loaded: "이 소스에서 트레이스를 이미 불러왔습니다",
    lockedHint: (bytes) => `이 소스 자체의 트레이스(${bytes})입니다. 다른 트레이스를 선택하려면 자물쇠를 클릭하세요.`,
    clear: "지우기",
    zip: "Zip",
    folder: "폴더",
    tracesHint: "‘트레이스 다운로드’로 받은 amprtrace zip 파일이나, 두 트레이스 파일이 있는 폴더를 선택하세요.",
    useOwn: "소스의 트레이스 사용",
    patchResult: (a, n) => a === "patch"
      ? `패치 완료: 트레이스 런타임이 설치되었고 인덱스에 파일 ${num(n)}개가 기록되었습니다.`
      : `패치 해제 완료: 릴리스 런타임이 돌아왔고 인덱스에 파일 ${num(n)}개가 기록되었습니다.`,
    removed: (files) => `이전 트레이스 파일을 삭제했습니다: ${files.join(", ")}.`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "LZ4 사용 방법",
    close: "닫기",
    warning:
      "실험적 기능이며 AMPR 게임 전용입니다. 원본 게임의 사본을 보관하세요.",
    body: () => (
      <>
        <h3>1. 패킹/언패킹</h3>
        <ol>
          <li>
            원본 게임과 <b>패킹/언패킹</b>, 그리고 트레이스(또는 프로필)를 선택하세요.
          </li>
          <li>패킹된 폴더나 일반 이미지로 쓰세요. 폴더는 <b>변환</b>에서 어떤 형식으로든 변환할 수 있습니다.</li>
          <li>
            패킹된 소스에서는 대신 <b>언패킹</b>이 표시되며, 개별 파일이 원래대로 돌아옵니다.
          </li>
          <li>게임의 ampr_emu가 번들 버전 이상이고 LZ4 팩을 읽을 수 있으면 패킹은 그대로 둡니다. 아니면 번들된 릴리스 ampr_emu를 설치합니다. LZ4 트레이스 런타임이나 ampr_emu가 없는 게임도 마찬가지입니다.</li>
        </ol>

        <h3>2. 트레이스</h3>
        <ol>
          <li>
            게임의 <b>ffpkg</b> 또는 <b>exfat</b>을 선택한 다음 <b>트레이스</b> → <b>패치</b>를 누르세요.
          </li>
          <li>fakelib 업데이터를 끄세요. 업데이터가 트레이스 런타임을 덮어씁니다.</li>
          <li>
            <span className="path">image_rw=</span>로 마운트하고 한 세션을 플레이한 다음 게임을 닫으세요.
          </li>
          <li>
            PS5 페이지에서 <b>트레이스 다운로드</b>를 누르거나 ampr_commands.bin과 ampr_emu.index를 복사하세요.
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    drives: "드라이브",
    location: "위치",
    parent: "상위 폴더",
    allDrives: "모든 드라이브",
    truncated: (n) => `처음 ${num(n)}개 항목만 표시됩니다.`,
    loading: "불러오는 중",
    noDrives: "드라이브가 없습니다.",
    noFolders: "여기에 폴더가 없습니다.",
    noImages: "여기에 폴더, PS5 이미지나 압축 파일이 없습니다.",
    noFiles: (ext) => `여기에 폴더나 ${dotted(ext, " 또는 ")} 파일이 없습니다.`,
    chooseFiles: (n) => `파일 ${num(n)}개 선택`,
    nothing: "여기에 폴더나 파일이 없습니다.",
    cancel: "취소",
    chooseFolder: "이 폴더 선택",
  },
  // common.tsx
  common: {
    folder: "폴더",
    pickFolder: "게임 폴더",
    pickImage: "게임 이미지 또는 압축 파일",
    pickImageFilter: "이미지 및 압축 파일",
    pickFolderButton: "폴더",
    pickImageButton: "이미지/압축 파일",
    source: "소스",
    emptyTitle: "게임 폴더, 이미지 또는 압축 파일을 선택하세요",
    emptyText: "PS5 게임 폴더 또는 그 게임의 이미지, 패키지나 압축 파일입니다.",
    accepted: "지원하는 소스",
    unknownTitle: "알 수 없는 게임",
    additionalContent: "추가 콘텐츠",
    reading: "소스 읽는 중",
    unreadable: "읽을 수 없는 소스",
    backport: "백포트",
    emulators: "에뮬레이터",
    declares: (fw) => `선언된 펌웨어 ${fw}`,
    fwUnknown: "실행 파일로는 펌웨어를 알 수 없음(읽을 수 있는 SDK 없음)",
    emulatorList: (names) => `에뮬레이터: ${names}`,
    otherEmulator: "기타",
    sdk: (v) => `SDK ${v}(선언됨)`,
    titleId: "타이틀 ID",
    version: "버전",
    firmware: "펌웨어",
    fwDeclared: "선언됨",
    fwBackport: "백포트",
    fwExecutables: "실행 파일",
    size: "크기",
    files: (n) => `파일 ${num(n)}개`,
    contentId: "콘텐츠 ID",
    noAmpr: "eboot.bin이 libSceAmpr를 가져오지 않음",
    runtime: {
      forge_release: "Forge 릴리스",
      forge_trace: "Forge 트레이스",
      other: "기타",
      none: "없음",
    },
    packed: "패킹됨",
    damaged: "손상된 팩",
    traced: "트레이스됨",
    plain: "일반",
    packedDetail: (files, volumes, percent) =>
      percent === null
        ? `파일 ${num(files)}개, 볼륨 ${num(volumes)}개`
        : `파일 ${num(files)}개, 볼륨 ${num(volumes)}개, 크기의 ${num(percent)}%`,
    journal: (size) => `저널 ${size}`,
    noJournal: "아직 저널 없음",
    runtimeIs: (label) => `런타임: ${label}`,
    index: {
      stale: "ampr_emu.index 오래됨: 변환 시 갱신",
      damaged: "ampr_emu.index 손상됨: 변환 시 다시 씀",
      missing: "ampr_emu.index 없음: 변환 시 작성",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `변환 시 번들된 ampr_emu ${version} 버전 설치`
        : a === "replace"
          ? `변환 시 번들된 ampr_emu ${version} 버전으로 런타임 교체`
          : `ampr_emu ${version} 버전: 변환 시 유지`,
    backportLibs: (exclusive, n) =>
      exclusive
        ? `fakelib2/(단독 마운트), 백포트 라이브러리 ${num(n)}개`
        : `fakelib/, 백포트 라이브러리 ${num(n)}개`,
    findings: "사전 점검 결과",
    dlcTitle: (n) => `내장 DLC(${num(n)})`,
    dlcLabel: "내장 DLC",
    dlcAll: (note) => `모든 DLC: ${note}.`,
    dlcIn: (folder, size) => `${folder}/ 안, ${size}`,
    dlcNoData: "DLC 에뮬레이터(dlc_emu.ini)로 잠금 해제: 권한만 있고 자체 데이터는 없음",
    dlcStatus: (status) => `DLC 에뮬레이터(dlc_emu.ini)로 잠금 해제, 상태: ${status || "설정 안 됨"}`,
    dlcMerged: "게임 폴더에 병합됨",
    backportFiles: "백포트 파일",
    emulatorsIn: "fakelib의 에뮬레이터",
    qr: (url) => `QR 코드: ${url}`,
    needsPassword: "이 압축 파일에는 비밀번호가 필요합니다.",
    enterPassword: "비밀번호 입력",
    inArchive: (ext, size) => `${ext} 압축 파일 안 · ${size}`,
    archive: "압축 파일",
    parts: (n) => `파트 ${num(n)}개`,
    archiveRoot: (root) => `게임 위치: ${root}`,
    archiveAtRoot: "게임이 압축 파일 루트에 있음",
    solid: "솔리드",
    encrypted: "비밀번호 보호",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "이 폴더를 삭제할까요?" : "이 이미지를 삭제할까요?"),
    text: (folder) =>
      folder
        ? "폴더와 그 안의 모든 항목이 영구적으로 삭제됩니다. 휴지통으로 이동하지 않으며 되돌릴 수 없습니다. 삭제가 중간에 멈추면 일부는 이미 삭제되었을 수 있습니다."
        : "이미지 파일이 영구적으로 삭제됩니다. 휴지통으로 이동하지 않으며 되돌릴 수 없습니다. 삭제가 중간에 멈추면 일부는 이미 삭제되었을 수 있습니다.",
    no: "아니요",
    yes: "예, 삭제",
    archiveTitle: "이 압축 파일을 삭제할까요?",
    archiveText:
      "압축 파일이 영구적으로 삭제됩니다. 휴지통으로 이동하지 않으며 되돌릴 수 없습니다. 분할 압축 파일의 다른 파트는 그대로 남습니다.",
  },
  // password.tsx
  password: {
    title: "이 압축 파일에는 비밀번호가 필요합니다",
    text: "열려면 비밀번호를 입력하세요.",
    wrong: "비밀번호가 올바르지 않습니다.",
    label: "비밀번호",
    cancel: "취소",
    unlock: "잠금 해제",
    checking: "확인 중…",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "포트 9021의 페이로드 로더에 연결할 수 없습니다. 먼저 오토로더(탈옥)를 실행한 다음 Dump Forge를 다시 여세요.",
      missing:
        "저장된 Dump Forge 사본을 찾을 수 없습니다(/data/dumpforge/ 또는 USB 드라이브). PC에서 한 번 로드하면 이 아이콘용 사본이 저장됩니다.",
      timeout:
        "Dump Forge를 시작했지만 30초가 지나도 응답이 없습니다. 아직 시작 중일 수 있습니다(대기 모드에서 깨어나는 콘솔은 느립니다).",
      stopping: "이전 Dump Forge가 아직 중지되는 중입니다(작업을 취소하고 정리하는 중). 잠시 기다린 후 다시 시도하세요.",
    },
    loadFailed: (e) => `페이지를 불러오지 못했습니다: ${e}`,
    noAnswer: "Dump Forge가 응답하지 않습니다",
    cantStart: "Dump Forge를 시작할 수 없습니다",
    retry: "다시 시도",
    starting: "Dump Forge 시작 중…",
    client: {
      unreachable:
        "Dump Forge에 연결할 수 없습니다. 페이로드가 아직 실행 중인지, 이 기기가 같은 네트워크에 있는지 확인하세요.",
      noPlatform: "Dump Forge가 PS5에서 실행 중인지 확인할 수 없습니다. 새로 고쳐 다시 시도하세요.",
      quitUnreachable: "Dump Forge를 중지하려 했지만 연결할 수 없습니다.",
      stillAnswering: "2분이 지나도 Dump Forge가 계속 응답합니다. PS5를 확인하세요.",
      gone: "Dump Forge가 더 이상 이 작업을 보고하지 않습니다(결과를 확인하기 전에 작업 기록에서 사라짐)",
    },
  },
  // target.tsx
  target: {
    format: {
      folder: { text: "이미지나 패키지를 일반 게임 폴더로 되돌립니다." },
      exfat: {
        text: "ffpkg로는 제대로 동작하지 않고 외장 드라이브 콘텐츠로만 실행되는 게임용입니다. Mac이나 PC에서도 열립니다.",
      },
      ffpkg: { tone: "good", text: "권장. PS5 고유의 파일 시스템(UFS2)입니다. 쓰기 가능한 마운트를 지원합니다." },
      ffpfs: {
        tone: "warn",
        text: "실험적. PS5의 PFS 파일 시스템이며 압축하지 않습니다. 파일 이름은 ASCII만 사용할 수 있습니다.",
      },
      ffpfsc: {
        tone: "warn",
        text: "실험적. 가장 작습니다. 이미지 하나를 감싼 압축 컨테이너이며 항상 읽기 전용으로 마운트됩니다.",
      },
      pkg: {
        text: "스토어 게임처럼 설치되고 네이티브 속도로 실행됩니다. kstuff, fpkg-enable, ppr-patch가 필요합니다.",
      },
      lz4: {
        tone: "warn",
        text: "실험적. 에셋을 LZ4 볼륨으로 패킹한 게임 폴더입니다. 게임이 실행되는 동안 번들된 AMPR 에뮬레이터(ampr_emu)가 압축을 풉니다. AMPR을 사용하는 게임 전용입니다.",
      },
    },
    innerFfpkg: { tone: "good", text: "권장. PS5 고유의 파일 시스템(UFS2)입니다." },
    level0: { text: "압축 없음: 모든 블록을 그대로 저장합니다. 파일 크기가 이미지와 같습니다." },
    levelDefault: {
      tone: "good",
      text: "권장. zlib 기본값입니다. 1은 약 3배 빠르게 빌드되지만 파일이 약 3% 커지고, 9는 거의 작아지지 않습니다.",
    },
    levelLow: {
      text: "6보다 빠르게 빌드되지만(1은 약 3배) 파일이 커집니다(1은 약 3%).",
    },
    levelHigh: {
      text: "6보다 거의 작아지지 않지만(9는 0.04%) 압축 시간이 길어지고(9는 약 25%) 그동안 모든 코어를 사용합니다.",
    },
    kraken: {
      fast: { label: "빠름", note: { tone: "good", text: "권장. 가장 빠른 빌드입니다." } },
      balanced: {
        label: "균형",
        note: {
          text: "‘빠름’보다 약 2.6% 작지만 압축 시간이 약 6배 걸리며, 그동안 모든 코어를 사용합니다.",
        },
      },
      smallest: {
        label: "최소 크기",
        note: {
          text: "‘빠름’보다 약 2.7% 작지만 압축 시간이 약 9배 걸리며, 그동안 모든 코어를 사용합니다.",
        },
      },
      stored: {
        label: "압축 안 함",
        note: {
          text: "패키지 크기가 게임과 비슷합니다. 압축된 패키지가 설치는 되지만 실행되지 않을 때 사용해 보세요(펌웨어 8.60, 10.40에서 보고됨).",
        },
      },
    },
    pkgFirmware: "펌웨어 11.60 이하에서만 동작합니다.",
    imageInside: "내부 이미지",
    imageInsideLabel: "ffpfsc 내부 이미지",
    compressionLevel: "압축 수준",
    compression: "압축",
    fullVerify: "전체 검증",
    fullVerifyHint: "모든 바이트를 다시 읽어 출력을 확인합니다. 시간이 더 걸립니다.",
    separator: "이름에는 폴더 구분자를 넣을 수 없습니다. 폴더는 ‘변경’으로 바꾸세요.",
    badName: "이름은 비워 둘 수 없으며, 점으로 시작하거나 점이나 공백으로 끝나거나 제어 문자를 포함할 수 없습니다.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `너무 깁니다: ${bytes}바이트, 최대 ${max}바이트(UTF-8).`
        : `너무 깁니다: 확장자 앞까지 ${bytes}바이트, 최대 ${max}바이트(UTF-8).`,
    pkgExt: (ext) => `확장자가 .${ext}이지만 fpkg는 .pkg로 저장됩니다. 확장자에 따라 드라이버가 결정됩니다.`,
    otherExt: (ext, target) =>
      `확장자가 .${ext}이지만 대상 형식은 ${target}입니다. 확장자에 따라 드라이버가 결정됩니다.`,
    extractInto: "이 폴더에 추출",
    saveInto: "이 폴더에 저장",
    output: "출력",
    change: "변경",
    naming: "이름 짓는 중…",
    chooseSourceFirst: "먼저 소스를 선택하세요",
    fileName: "파일 이름",
    savedAs: (name, dir) => `저장 이름: ${name}, 위치: ${dir}`,
    inDir: (dir) => `위치: ${dir}`,
    generate: "내용을 바탕으로 이름 생성",
    generatePattern: "[게임 이름]-[타이틀 ID]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `v${v} 업데이트가 있습니다`,
    update: "업데이트",
    later: "나중에",
    updating: "업데이트 중…",
    updatingNote: "새 버전을 다운로드하는 중입니다. 앱이 자동으로 다시 시작됩니다.",
    downloading: "브라우저에서 다운로드 중입니다. 압축을 풀고 이 사본을 교체하세요.",
    failed: (v) => `v${v}(으)로 업데이트하지 못했습니다`,
    timeout: (v) => `v${v}이(가) 1분 안에 시작되지 않았습니다. 다운로드되었다면 Dump Forge를 중지한 뒤 다시 열어 업데이트를 마치세요.`,
    retry: "다시 시도",
    close: "닫기",
  },
  // api.ts: prettyDuration's units
  duration: { s: (s) => `${s}초`, ms: (m, s) => `${m}분 ${s}초`, hm: (h, m) => `${h}시간 ${m}분` },
};

export default ko;
