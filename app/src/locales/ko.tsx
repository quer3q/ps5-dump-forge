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
    screens: "화면",
    convert: "변환",
    active: (n) => `, ${num(n)}개 진행 중`,
    edit: "편집",
    pack: "패킹",
    storage: "저장소",
    connect: "연결",
    disconnected: "연결 안 됨",
    connected: (n: number) => `연결됨 (기기 ${num(n)}대)`,
    about: "정보",
    prevTab: "이전 탭 (L2)",
    nextTab: "다음 탭 (R2)",
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
    quitting: "PS5 Dump Forge 종료 중…",
    couldntStop: "Dump Forge를 중지하지 못했습니다",
    close: "닫기",
    tryAgain: "다시 시도",
    stopped: "Dump Forge가 중지되었습니다",
    startAgain: (ps5) => (ps5 ? "계속하려면 PS5에서 Dump Forge 타일을 다시 여세요." : "계속하려면 같은 ps5-dump-forge serve 명령을 다시 실행하세요."),
    confirmStop: (web: boolean) => (web ? "Dump Forge를 중지할까요?" : "PS5 Dump Forge를 종료할까요?"),
    stopNote: (web, ps5) => (!web ? "실행 중인 작업이 없습니다." : ps5 ? "다시 시작하려면 PS5에서 Dump Forge 타일을 여세요." : "다시 시작하려면 같은 ps5-dump-forge serve 명령을 다시 실행하세요."),
    peerBusy: "페어링된 기기가 이 컴퓨터의 파일을 읽거나 쓰고 있습니다. 종료하면 그 기기의 전송이 중단됩니다.",
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
      "이 게임의 에셋은 LZ4로 패킹되어 있고 LZ4 매니페스트에 백포트가 포함되어 있습니다. 먼저 ‘패킹’ 탭에서 언패킹한 다음 여기에서 백포트를 제거하세요.",
    zstdBackport:
      "이 게임의 에셋은 zstd로 패킹되어 있고 zstd 인덱스에 백포트가 포함되어 있습니다. 먼저 ‘패킹’ 탭에서 언패킹한 다음 여기에서 백포트를 제거하세요.",
    unreadable: "빌드할 수 없습니다: 이 소스를 읽지 못했습니다(‘소스’ 참고).",
    patchBlocked: "빌드할 수 없습니다: 패치 패키지라서 업데이트로 바뀐 부분만 들어 있습니다. 파일을 추출하려면 ‘편집’에서 여세요.",
    openInEdit: "‘편집’에서 열기",
    noContentId: "fpkg로 빌드할 수 없습니다: 이 게임의 param.json에 패키지에 필요한 Content ID가 없습니다.",
    blocked: (first, more) =>
      `빌드할 수 없습니다: ${first}${more > 0 ? ` (외 ${num(more)}개, ‘소스’ 참고)` : ""}.`,
    amprZstdRuntime: "이 변환은 이 팩에 필요한 ampr-emu-ng를 fakelib/libSceAmpr.sprx에 설치합니다.",
    amprNgReplaced: (version) => `이 게임의 ampr-emu-ng를 ampr_emu ${version} 버전으로 바꿉니다.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `이 게임은 AMPR을 사용하지만 ampr_emu가 없습니다. 변환하면 번들된 ampr_emu ${version} 버전을 설치합니다.`;
      if (a === "trace") return `변환하면 이 게임의 ampr_emu가 번들된 ampr_emu ${version} 버전으로 바뀝니다.`;
      if (a === "replace")
        return own
          ? `변환하면 이 게임의 ampr_emu ${own} 버전이 번들된 ampr_emu ${version} 버전으로 바뀝니다.`
          : `변환하면 버전을 알 수 없는 이 게임의 ampr_emu가 번들된 ampr_emu ${version} 버전으로 바뀝니다.`;
      return own
        ? `이 게임의 ampr_emu ${own} 버전은 번들된 ${version} 버전과 같거나 더 새로우므로 변환해도 그대로 유지됩니다.`
        : `이 게임에는 이미 번들된 ampr_emu ${version} 버전이 있어 변환해도 그대로 유지됩니다.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index가 이 게임의 파일과 맞지 않습니다" : state === "damaged" ? "ampr_emu.index를 읽을 수 없습니다" : "이 게임에는 ampr_emu.index가 없습니다"}. 변환하면 새 인덱스를 씁니다.`,
    amprPkgIndex: "패키지에는 담긴 파일로 만든 새 ampr_emu.index가 들어갑니다.",
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
      zstd: { tagline: "AMPR를 쓰는 게임용, ampr-emu-ng 사용", cells: ["로딩 최대 80% 빠름", "게임 크기를 최대 절반으로"] },
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
      ["전송", "SMB, FTP 또는 NFS로 PS5, 컴퓨터, NAS 사이에서 파일을 보내고 게임을 변환합니다."],
      ["편집", "게임 파일을 폴더처럼 수정하고, 빌드하기 전에 게임의 커버, 펌웨어, 백포트, DLC, 파일 이름 문제를 확인합니다."],
      ["스트리밍", "어떤 원본에서 어떤 대상으로든 임시 복사본 없이 처리합니다. ffpfsc 안의 이미지나 아카이브 안의 게임도 마찬가지입니다."],
      ["아카이브", "게임이 든 .zip, .7z, .rar 를 폴더처럼 탐색, 변환, 패킹합니다."],
      ["USB 및 SD 카드", "exFAT 또는 FAT32로 포맷된 USB 메모리와 카드에 기록합니다."],
      ["패킹", "zstd 또는 LZ4(실험적) 에셋 팩을 스마트 복구와 함께 만듭니다."],
      ["저장소", "게임 라이브러리와 이름 변경·이동·삭제를 할 수 있는 파일 탐색기."],
      ["안전", "요청하지 않으면 게임을 절대 변경하지 않으며, 불필요한 파일은 건너뛰고 잘못된 파일 이름은 미리 알려 줍니다."],
    ],
    about: "정보",
    compare: "형식 비교",
    compareLabel: "형식 비교",
    format: "형식",
    credits: "크레딧",
    upstream: (app) =>
      `업스트림 수치: ampr_emu와 Lazy_AMPR 개발자가 보고한 값이며 ${app}에서 측정한 값이 아닙니다. 게임에 따라 달라집니다. ampr_emu 0.4.2.1은 업스트림 테스트 빌드이며, 알려진 문제로 일부 게임이 저장 중에 비정상 종료됩니다.`,
    bundled: "LZ4로 패킹된 폴더는 drakmor의 ampr_emu 0.4.2.1(GPL-3.0)로 실행되며, 수정 없이 번들되어 있습니다. 소스 코드:",
    lazy: "LZ4 패킹은 Nazky의 Lazy_AMPR를 기반으로 합니다:",
    ng: "zstd 및 LZ4 팩은 ampr-emu-ng에서 실행됩니다. 소스:",
    logo: "로고는 Xxcco가 만들었습니다:",
    unrar:
      ".rar 파일은 함께 제공되는 UnRAR 라이브러리(Alexander Roshal 제작)로 읽습니다(압축 해제만, UnRAR 라이선스 적용).",
    language: "언어",
    system: "시스템",
    notifications: "작업이 끝나면 알림 표시",
    notificationsHint: "변환, 패킹, 전송 작업의 알림을 콘솔에 표시합니다.",
    notificationsFailed: "알림 설정을 변경하지 못했습니다.",
  },
  // Edit.tsx
  edit: {
    details: "세부 정보",
    deleting: "소스를 삭제하는 중",
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
      "**게임 폴더**나 **이미지**를 열고 USB 메모리의 폴더처럼 수정하세요. **파일을 추가하거나 교체하고**(끌어다 놓거나 ‘추가’ 사용), **이름을 바꾸고 삭제할** 수 있습니다.",
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
    actions: "작업",
    items: (n) => `항목 ${num(n)}개`,
    inFolder: (dirs, files) => `폴더 ${num(dirs)}개, 파일 ${num(files)}개`,
    matches: (n) => `${num(n)}개 일치`,
    emptyFolder: "이 폴더는 비어 있습니다.",
    open: (name) => `${name} 열기`,
    archiveNote: "압축 파일은 둘러볼 수 있지만 편집할 수는 없습니다. 수정하려면 먼저 폴더나 이미지로 변환하세요.",
    nasNote: "NAS의 게임은 둘러보고 추출할 수 있지만 편집할 수는 없습니다. 변경하려면 먼저 이 기기로 변환하거나 추출하세요.",
    patchNote: "패치 패키지입니다. 업데이트로 바뀐 부분만 들어 있으며, 온전히 들어 있는 파일만 표시됩니다. 둘러보고 추출할 수 있지만 편집할 수는 없습니다.",
    peerNote: "다른 Forge의 게임은 둘러보고 추출할 수 있지만 편집할 수는 없습니다. 변경하려면 먼저 이 기기로 변환하거나 추출하세요.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "파일 추가",
    addFolder: "폴더 추가",
    pickAddFiles: "추가할 파일 선택",
    pickAddFolder: "추가할 폴더 선택",
    peerRoot: "기기 전체는 추가할 수 없습니다. 그 안의 폴더를 선택하세요.",
    renameItem: (name) => `${name} 이름 바꾸기`,
    removeItem: (name) => `${name} 삭제`,
    renameTip: "이름 바꾸기(F2)",
    removeTip: "삭제(Delete 키)",
    removeTipMac: "삭제(fn+Delete 키)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extractItem: (name: string) => `${name} 추출`,
    extractTip: "선택한 폴더로 복사",
    extractFolder: "폴더 추출",
    extractFolderTip: "표시된 폴더를 선택한 폴더로 복사",
    extractGameTip: "게임 전체를 선택한 폴더로 복사",
    extractNotOnDisk: "이 폴더는 보류 중인 변경 사항에만 있습니다. 추출하려면 변경 사항을 적용하세요.",
    pickExtract: "추출할 위치 선택",
    extractPending: "보류 중인 변경 사항 없이 현재 상태의 파일을 추출합니다.",
    nameEmpty: "이름을 입력하세요.",
    nameBad: "이름에는 / 또는 \\를 넣을 수 없으며, . 또는 ..일 수도 없습니다.",
    nameTaken: (name) => `‘${name}’ 항목이 이미 여기에 있습니다.`,
    markNew: "신규",
    markReplaced: "교체됨",
    markRenamed: "이름 변경됨",
    wasAt: (path) => `이전 위치: ${path}`,
    sizeLater: "목록에 다 표시할 수 없을 만큼 많습니다. 나머지는 변경 사항을 적용할 때 읽습니다",
    dropInto: (folder) => `놓으면 ${folder}에 추가`,
    droppedGone: (path) => `${path}: 끌어다 놓은 파일이 더 이상 없습니다`,
    pending: (n) => `아직 적용하지 않은 변경 ${num(n)}개`,
    undo: "실행 취소",
    undoTip: "마지막 작업과 그 변경을 모두 실행 취소",
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
    nothingToChange: "바꿀 것이 없습니다. 게임은 그대로입니다.",
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
    uploadingFile: (i, n) => `파일 ${num(i)}/${num(n)} 업로드 중`,
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
      install: "설치 중",
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
      install: "설치",
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
    packTag: {
      lz4Pack: (level: string) => `LZ4 패킹 · ${level}`,
      lz4Unpack: "LZ4 언패킹",
      zstdPack: (level: string) => `zstd 패킹 · ${level}`,
      level: (n: number) => `레벨 ${num(n)}`,
      zstdUnpack: "zstd 언패킹",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "편집",
    editRebuild: "편집 · 다시 빌드",
    transfer: "전송",
    extract: "추출",
    job: (id) => `작업 ${id}`,
    done: "완료",
    cancelled: "취소됨",
    failed: "실패",
    cancelling: "취소 중…",
    queued: "대기 중",
    dismiss: "닫기",
    retry: "다시 시도",
    retrying: "시작 중",
    stopJob: "중지할까요?",
    cancel: "취소",
    stopHint: "4초 안에 ‘중지할까요?’를 다시 누르면 작업이 중지됩니다.",
    remoteLabel: (name, device) => `${name}, ${device}의 작업`,
    remoteTitle: (device) => `${device}에서 실행 중이며 이 기기의 파일을 사용합니다. 여기서는 취소만 할 수 있습니다`,
    from: "소스: ",
    left: (d) => `${d} 남음`,
    inStep: (time) => `이 단계에서 ${time} 경과`,
    result: (bytes, n) => `${bytes}, 파일 ${num(n)}개 →`,
    outputPath: "출력 경로",
    sourceDeleted: (v) => `소스 삭제됨: ${v}`,
    sourceKept: (v) => `소스 유지됨: ${v}`,
    sourcePartlyDeleted: (v) => `소스 일부 삭제됨: ${v}`,
    checks: "확인 항목",
    verifiedPlain: (n) => `검증 완료: 확인 항목 ${num(n)}개 통과`,
    verifiedFull: (n) => `전체 검증 통과: 확인 항목 ${num(n)}개`,
    verifiedFast: (n) => `빠른 검증 통과: 확인 항목 ${num(n)}개`,
    coverage: (checked, total, n) => `샘플 ${num(n)}개로 ${total} 중 ${checked} 확인`,
    cantBuild: "이 게임을 빌드할 수 없습니다:",
    cantEdit: "변경 사항을 적용할 수 없습니다:",
    cantExtract: "추출할 수 없습니다:",
    cantTransfer: "전송할 수 없습니다:",
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
    library: "라이브러리",
    storageTip: "콘솔 내부 저장소(/data) 여유 공간",
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
    deleting: (n) => `${n} 삭제 중`,
    lz4Tip: "LZ4 에셋 팩",
    zstdTip: "zstd 에셋 팩",
    showAs: "보기 방식",
    kinds: "종류별 보기",
    all: "전체",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n, path) => (
      <>
        게임 <b>{num(n)}</b>개, 위치: {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "저장소 보기",
    library: "라이브러리",
    files: "파일 탐색기",
    title: "파일 탐색기",
    pickTitle: "탐색할 폴더 선택",
    pickPrompt: "파일을 탐색할 폴더를 선택하세요",
    choose: "폴더 선택",
    roots: "드라이브",
    location: "위치",
    folders: "폴더",
    up: "위로",
    upTip: "상위 폴더로",
    refresh: "새로 고침",
    filter: "이름으로 필터",
    filterLabel: "이 폴더를 이름으로 필터",
    inFolder: (dirs, files) => `폴더 ${num(dirs)}개, 파일 ${num(files)}개`,
    matches: (n) => `${num(n)}개 일치`,
    firstOf: (max, n) => `${num(n)}개 중 처음 ${num(max)}개`,
    truncated: "이 폴더에는 항목이 10,000개를 넘게 있습니다. 처음 10,000개만 표시합니다.",
    name: "이름",
    size: "크기",
    emptyFolder: "이 폴더는 비어 있습니다.",
    noMatch: (f) => `‘${f}’에 해당하는 항목이 없습니다.`,
    noRoots: "탐색할 수 있는 드라이브가 없습니다.",
    cantList: "이 폴더를 열 수 없습니다:",
    link: "링크",
    linkTip: "링크: 열 수는 있지만 여기서 이름 바꾸기, 이동, 삭제는 할 수 없습니다",
    mounted: "마운트됨",
    mountedTip: "ShadowMount+가 마운트했습니다: 삭제, 이름 변경 또는 이동하려면 게임을 종료하고 ShadowMount+를 끄세요",
    mountedNote: "마운트됨으로 표시된 이미지는 ShadowMount+가 사용 중입니다. 삭제, 이름 변경 또는 이동하려면 게임을 종료하고 ShadowMount+를 끄세요.",
    open: (n) => `${n} 열기`,
    more: (n) => `${n}에 대한 추가 작업`,
    rename: (n) => `${n} 이름 바꾸기`,
    renameTip: "이름 바꾸기(F2)",
    move: (n) => `${n} 이동`,
    moveTip: "다른 폴더로 이동",
    moveTitle: (n) => `${n}을(를) 옮길 폴더`,
    remove: (n) => `${n} 삭제`,
    removeTip: "삭제(Delete 키)",
    removeTipMac: "삭제(fn+Delete 키)",
    cantDelete: "삭제할 수 없습니다:",
    cantRename: "이름을 바꿀 수 없습니다:",
    cantMove: "이동할 수 없습니다:",
    sameFolder: "이미 그 폴더에 있습니다.",
    deleted: (path) => `삭제됨: ${path}`,
    renamed: (path) => `이름 변경됨: ${path}`,
    moved: (path) => `이동됨: ${path}`,
    devices: "기기",
    thisDevice: "이 기기",
    otherDevice: "‘이동’은 한 기기 안에서만 됩니다. 다른 기기로 복사하려면 그 기기의 탭으로 끌어다 놓으세요.",
    dropCopy: (folder) => `놓으면 ${folder}에 복사합니다`,
    dropMove: (folder) => `놓으면 ${folder}(으)로 이동합니다`,
    cantCopy: "복사할 수 없습니다:",
    copyNoFolder: "먼저 그 기기에서 폴더를 여세요.",
    copyOne: (name, to) => `${name}을(를) ${to}에 복사하는 중입니다. 전송은 ‘연결’에 표시됩니다.`,
    copyMany: (n, to) => `항목 ${num(n)}개를 ${to}에 복사하는 중입니다. 전송은 ‘연결’에 표시됩니다.`,
    openConnect: "‘연결’ 열기",
  },
  // Pack.tsx
  pack: {
    title: "패킹",
    help: "패킹 사용 방법",
    engine: "팩 종류",
    engines: { zstd: "zstd", lz4: "LZ4" },
    actions: { pack: "패킹", unpack: "언패킹" },
    info: (e, packed) =>
      e === "zstd"
        ? packed
          ? "zstd 팩을 일반 파일로 되돌립니다."
          : "게임 에셋을 zstd 볼륨으로 패킹합니다."
        : packed
          ? "LZ4 팩을 일반 파일로 되돌립니다."
          : "일부 게임은 시작되지 않을 수 있습니다. 게임 에셋을 LZ4 볼륨으로 패킹합니다.",
    noAmprGame: (e) => `이 게임은 AMPR을 사용하지 않으므로 ${e === "zstd" ? "zstd" : "LZ4"} 팩을 쓸 수 없습니다.`,
    patch: "패치 패키지는 패킹할 수 없습니다.",
    additionalContent: "추가 콘텐츠(eboot.bin 없음)는 패킹할 수 없습니다.",
    unreadable: "시작할 수 없습니다: 소스를 읽지 못했습니다(‘소스’ 참고).",
    blocked: (a, first, more) =>
      `${a === "pack" ? "패킹" : "언패킹"}할 수 없습니다: ${first}${more > 0 ? ` (외 ${num(more)}개, ‘소스’ 참고)` : ""}.`,
    target: "대상",
    packFolder: (e) => `권장. 가장 빠르고 지연이 가장 적습니다: 에셋이 ${e === "zstd" ? "zstd" : "LZ4"} 팩에 담긴 새 게임 폴더입니다.`,
    levels: { fast: "빠름", medium: "보통", max: "최대" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "권장. zstd 5: 빠르고 크기도 적당합니다." },
        medium: { text: "zstd 9: 더 작고 더 느립니다." },
        max: { text: "zstd 19: 가장 작습니다. 훨씬 느리고 메모리를 더 씁니다." },
      },
      lz4: {
        fast: { tone: "good", text: "권장. LZ4 기본값: 가장 빠릅니다." },
        medium: { text: "LZ4 HC 9: 더 작고 더 느립니다." },
        max: { text: "LZ4 HC 12: 가장 작고 가장 느립니다." },
      },
    },
    runtime: "런타임",
    runtimeNotes: {
      ng: { tone: "good", text: "권장. zstd와 LZ4 팩을 읽습니다." },
      ampr_emu: { text: "업스트림 ampr_emu." },
    },
    ngNote: "ampr-emu-ng가 필요합니다: 패킹하면 fakelib/에 설치됩니다. ShadowMount+의 fakelib 업데이트를 끄세요. 켜 두면 런타임이 교체됩니다.",
    installsRuntime: (version) => `여기에는 ampr_emu가 없습니다: 패킹하면 ampr_emu ${version} 버전이 설치됩니다.`,
    freshIndex: (state) =>
      `${state === "stale" ? "ampr_emu.index가 오래되었습니다" : state === "damaged" ? "ampr_emu.index가 손상되었습니다" : "ampr_emu.index가 없습니다"}: 패킹하면 새로 씁니다.`,
    direct: "이미지에 바로 패킹합니다. 게임은 두 번 읽습니다.",
    unpackFolder: "일반 파일로 된 새 게임 폴더입니다.",
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
    source: "소스",
  },
  // PackHelp.tsx
  packHelp: {
    title: "패킹 사용 방법",
    close: "닫기",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>게임 에셋을 zstd 볼륨으로 패킹해 폴더나 이미지에 씁니다. AMPR을 사용하는 게임 전용입니다.</li>
          <li><b>빠름</b>(zstd 5)은 빠르고, <b>보통</b>(9)과 <b>최대</b>(19)는 더 작지만 더 느립니다.</li>
          <li>패킹된 게임에는 <b>언패킹</b>이 표시되며, 일반 파일로 되돌립니다.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>일부 게임은 시작되지 않을 수 있습니다.</b> 사본을 보관하세요.</li>
          <li>LZ4 볼륨으로 패킹해 폴더나 이미지에 씁니다: <b>빠름</b>(LZ4), <b>보통</b>(HC 9), <b>최대</b>(HC 12).</li>
          <li>런타임: <b>ampr-emu-ng</b>(기본값) 또는 업스트림 <b>ampr_emu</b>.</li>
          <li>패킹된 게임에는 <b>언패킹</b>이 표시됩니다.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>zstd와 LZ4 팩을 읽습니다. 패킹하면 fakelib/에 설치됩니다.</li>
          <li>ShadowMount+의 fakelib 업데이트를 끄세요. 켜 두면 런타임이 교체되어 게임이 팩을 읽지 못합니다.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "기기",
    thisDevice: "이 기기",
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
    chooseFiles: (n) => (n === 0 ? "파일 선택" : `파일 ${num(n)}개 선택`),
    nothing: "여기에 폴더나 파일이 없습니다.",
    cantList: "이 폴더를 열 수 없습니다:",
    cancel: "취소",
    chooseFolder: "이 폴더 선택",
  },
  // common.tsx
  common: {
    folder: "폴더",
    pickButton: "선택",
    pickAny: "게임 폴더, 이미지 또는 압축 파일",
    source: "소스",
    emptyTitle: "게임 폴더, 이미지 또는 압축 파일을 선택하세요",
    accepted: "지원하는 소스",
    unknownTitle: "알 수 없는 게임",
    patch: "패치",
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
      forge_trace: "이전 LZ4 트레이스 런타임",
      zstd: "ampr-emu-ng",
      other: "기타",
      none: "없음",
    },
    packed: "패킹됨",
    zstdKept: "zstd 에셋 팩: 변환해도 유지됩니다",
    damaged: "손상된 팩",
    plain: "일반",
    packedDetail: (files, volumes, percent) =>
      percent === null
        ? `파일 ${num(files)}개, 볼륨 ${num(volumes)}개`
        : `파일 ${num(files)}개, 볼륨 ${num(volumes)}개, 크기의 ${num(percent)}%`,
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
        : "이미지 파일이 영구적으로 삭제됩니다. 휴지통으로 이동하지 않으며 되돌릴 수 없습니다.",
    no: "아니요",
    yes: "예, 삭제",
    archiveTitle: "이 압축 파일을 삭제할까요?",
    archiveText:
      "압축 파일이 영구적으로 삭제됩니다. 휴지통으로 이동하지 않으며 되돌릴 수 없습니다. 분할 압축 파일의 다른 파트는 그대로 남습니다.",
    fileTitle: "이 파일을 삭제할까요?",
    fileText: "파일이 영구적으로 삭제됩니다. 휴지통으로 이동하지 않으며 되돌릴 수 없습니다.",
  },
  // replace.tsx
  replace: {
    title: (n) => `항목 ${num(n)}개가 이미 그곳에 있습니다`,
    inFolder: (folder) => <>{folder} 안:</>,
    more: (n) => `외 ${num(n)}개`,
    text: "‘교체’를 누르면 새 항목이 그 자리를 대신합니다. 기존 파일은 휴지통을 거치지 않고 완전히 사라지며 되돌릴 수 없습니다. 같은 이름의 폴더 위에 놓은 폴더는 병합되어, 이름이 같은 파일은 교체되고 나머지는 그대로 남습니다.",
    cancel: "취소",
    replace: "교체",
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
      uploadIncomplete: "업로드가 완료되지 않았습니다.",
    },
  },
  // target.tsx
  target: {
    format: {
      folder: { text: "이미지나 패키지를 일반 게임 폴더로 되돌립니다." },
      exfat: {
        text: "ffpkg로는 제대로 동작하지 않고 외장 드라이브 콘텐츠로만 실행되는 게임용입니다. Mac이나 PC에서도 열립니다.",
      },
      ffpkg: { text: "PS5 고유의 파일 시스템(UFS2)입니다. 쓰기 가능한 마운트를 지원합니다." },
      ffpfs: {
        text: "PS5의 PFS 파일 시스템이며 압축하지 않습니다. 파일 이름은 ASCII만 사용할 수 있습니다.",
      },
      ffpfsc: {
        text: "가장 작습니다. 이미지 하나를 감싼 압축 컨테이너이며 항상 읽기 전용으로 마운트됩니다.",
      },
      pkg: { tone: "good", text: "권장. 네이티브 게임처럼 실행되지만 펌웨어가 허용하는 경우에만 가능합니다." },
      lz4: {
        tone: "warn",
        text: "실험적. 에셋을 LZ4 볼륨으로 패킹한 게임 폴더입니다. 게임이 실행되는 동안 번들된 AMPR 에뮬레이터(ampr_emu)가 압축을 풉니다. AMPR을 사용하는 게임 전용입니다.",
      },
    },
    innerFfpkg: { text: "PS5 고유의 파일 시스템(UFS2)입니다." },
    innerExfat: { tone: "good", text: "권장. 압축 컨테이너 안에서 가장 안정적인 이미지입니다." },
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
    imageInside: "내부 이미지",
    imageInsideLabel: "ffpfsc 내부 이미지",
    compressionLevel: "압축 수준",
    compression: "압축",
    fullVerify: "전체 검증",
    fullVerifyHint: "모든 바이트를 다시 읽어 출력을 확인합니다. 시간이 더 걸립니다.",
    deleteSource: "소스 삭제",
    deleteSourceHint: "되돌릴 수 없습니다. 소스는 변환에 성공한 뒤에만 삭제됩니다.",
    deleteSourceNas: "NAS에 있는 게임은 여기서 삭제할 수 없습니다.",
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
    updatingNote: "새 버전을 다운로드하는 중입니다. 이 페이지가 자동으로 새 버전으로 다시 로드됩니다.",
    downloading: "브라우저에서 다운로드 중입니다. 압축을 풀고 이 사본을 교체하세요.",
    failed: (v) => `v${v}(으)로 업데이트하지 못했습니다`,
    timeout: (v) => `v${v}이(가) 1분 안에 시작되지 않았습니다. 다운로드되었다면 Dump Forge를 중지한 뒤 다시 열어 업데이트를 마치세요.`,
    retry: "다시 시도",
    close: "닫기",
  },
  // api.ts: prettyDuration's units
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "이 기기",
    howItWorks: "연결 방법",
    ipAddress: "IP 주소",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? `마지막 접속: ${num(n)}분 전`
        : unit === "h"
          ? `마지막 접속: ${num(n)}시간 전`
          : unit === "d"
            ? `마지막 접속: ${num(n)}일 전`
            : "마지막 접속: 방금",
    listenError: (e) => `다른 기기가 이 기기에 연결할 수 없습니다: ${e}`,
    unavailable: (e) => `연결을 사용할 수 없습니다: ${e}`,
    loading: "불러오는 중",
    discover: "자동 검색",
    stop: "중지",
    fileTransfer: "파일 전송",
    devices: "기기",
    show: "표시",
    all: "전체",
    onlineOf: (n, total) => `${num(total)}대 중 ${num(n)}대 온라인`,
    sendFilesTo: (name) => `${name}에 파일 보내기`,
    details: (name) => `${name} 세부 정보`,
    userIs: (user) => `사용자 ${user}`,
    nearby: "주변",
    nearbyNone: "새 기기를 찾지 못했습니다",
    addDevice: "기기 추가",
    addressExample: "192.168.1.20 또는 nas.local",
    addByProtocol: "또는 프로토콜로 추가:",
    savedEmpty: "아직 저장된 기기가 없습니다",
    savedEmptyHint:
      "‘자동 검색’은 이 네트워크에서 Forge와 네트워크 스토리지를 찾습니다. 기기를 페어링하고 그 기기에서 허용하거나 NAS를 저장하세요.",
    online: "온라인",
    offline: "오프라인",
    computer: "컴퓨터",
    address: "주소",
    keyId: "키 ID",
    lastSeen: "마지막 접속",
    status: "상태",
    never: "없음",
    unknown: "알 수 없음",
    forget: "등록 해제",
    forgetNote: "‘등록 해제’는 이 기기에서만 페어링을 없앱니다. 다시 사용하려면 다시 페어링하세요.",
    close: "닫기",
    cancel: "취소",
    pairTitle: "기기 페어링",
    pair: "페어링",
    connectTo: "연결",
    probing: (address) => `${address} 확인 중`,
    pairing: "페어링 중",
    waitingFor: (address) => `${address}에서 페어링을 허용하기를 기다리는 중`,
    checkNumber: "확인 번호",
    checkHint: "다른 기기에 표시되는 번호와 같아야 합니다.",
    denied: "다른 기기가 페어링을 거부했습니다.",
    timedOut: "다른 기기에서 제시간에 페어링을 허용하지 않았습니다.",
    pairBusy: "다른 기기에 대기 중인 페어링 요청이 있습니다. 1분 뒤에 다시 시도하세요.",
    alreadyPaired: "그 기기에 이 기기가 이미 저장되어 있습니다. 먼저 그 기기에서 등록 해제한 뒤 다시 페어링하세요.",
    tooManyAttempts: "거부된 요청이 너무 많습니다. 10분 뒤에 다시 시도하세요.",
    pairPromptTitle: (name) => `${name}와(과) 페어링할까요?`,
    pairPromptBody: (name, platform, ip) =>
      `${name}(${platform}, ${ip})이(가) 이 기기와 페어링하려고 합니다. 페어링된 기기는 이 기기의 파일을 읽고 새 파일을 추가할 수 있습니다.`,
    pairPromptCheck: (name) => `${name}에 같은 확인 번호가 표시될 때만 허용하세요.`,
    allow: "허용",
    deny: "거부",
    notPaired: "다른 기기가 이 기기를 더 이상 알지 못합니다. 다시 페어링하세요.",
    searching: "이 네트워크를 검색하는 중",
    forge: "Forge",
    save: "저장",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `${protocol} NAS 추가`,
    nasEditTitle: (protocol) => `${protocol} NAS 편집`,
    nasName: "이름",
    nasAddressHint: "IP 주소 또는 호스트 이름. 포트가 445가 아니면 :포트를 붙이세요",
    nasUser: "사용자 이름",
    nasUserHint: "비워 두면 게스트로 접속합니다",
    ftpAddressHint: "IP 주소 또는 호스트 이름. 포트가 21이 아니면 :포트를 붙이세요",
    nfsAddressHint: "IP 주소 또는 호스트 이름. 포트가 2049가 아니면 :포트를 붙이세요",
    ftpUserHint: "비워 두면 익명으로 접속합니다",
    ftpNote: "FTP는 로그인 정보를 포함해 암호화되지 않습니다.",
    nfsNote: "NFS에는 로그인이 없습니다. Forge는 root로 접속하므로(NFSv4.1, 안 되면 NFSv3) 서버의 내보내기(export)가 이 기기를 허용해야 합니다. NFS는 암호화되지 않습니다.",
    nasPassword: "비밀번호",
    nasPasswordKeep: "비워 두면 저장된 비밀번호를 유지합니다",
    guest: "게스트",
    passwordSaved: "저장됨",
    passwordNone: "없음",
    edit: "편집",
    nasForgetNote: "‘등록 해제’는 이 NAS와 저장된 비밀번호를 이 기기에서 없앱니다.",
    pickSendFiles: "보낼 파일",
    pickSendFolder: "보낼 폴더",
    pickSaveTo: "저장할 위치",
    thisBrowser: "이 브라우저",
    browserHint: "이 휴대폰이나 컴퓨터의 파일을 이 기기의 폴더에 넣고, 이 기기의 파일을 이 브라우저로 저장합니다.",
    uploadTo: "업로드할 폴더",
    uploadPick: "업로드할 파일 선택",
    uploading: (name, i, n) => `${name} 업로드 중(${num(i)}/${num(n)})`,
    uploaded: "업로드가 끝났습니다",
    uploadSkipped: (names) => `업로드하지 않았습니다. 그곳에 같은 이름의 폴더나 링크가 있습니다: ${names}.`,
    uploadedBefore: (done, n) => `파일 ${num(n)}개 중 ${num(done)}개를 업로드했으며 이 기기에 남아 있습니다.`,
    cantStart: "전송을 시작할 수 없습니다:",
    cantUpload: "업로드할 수 없습니다:",
    pickDownload: "다운로드할 파일",
    transferIntro: "어느 기기의 파일이나 폴더든 이 기기, 페어링된 기기 또는 NAS의 폴더로 보냅니다. 전송은 작업 목록에서 작업으로 실행됩니다.",
    send: "보내기",
    sendWhat: "보낼 항목",
    sendTo: "보낼 위치",
    sendNeedsDevice: "먼저 다른 기기를 연결하세요. Forge를 페어링하거나 NAS를 추가하세요.",
    nothingChosen: "아직 선택한 항목이 없습니다",
    chooseFiles: "파일 선택",
    chooseFolder: "폴더 선택",
    startTransfer: "시작",
    uploadHere: "이 기기로 업로드",
    downloadHere: "이 기기에서 다운로드",
    consoleNoFiles: "PS5 브라우저에서는 파일을 선택할 수 없습니다. 휴대폰이나 컴퓨터에서 이 페이지를 여세요.",
    wholeRoot: "기기, 공유 폴더 또는 드라이브 전체는 보낼 수 없습니다. 그 안의 폴더를 선택하세요.",
    helpPhone: "QR 코드를 스캔하면 휴대폰에서 이 페이지가 열립니다. 그곳에서 ‘파일 전송’의 ‘이 기기로 업로드’와 ‘이 기기에서 다운로드’로 휴대폰의 파일을 옮길 수 있습니다.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "‘연결’은 같은 네트워크에 있는 PS5와 컴퓨터의 Dump Forge를 서로 이어 줍니다.",
      "페어링하려면 ‘자동 검색’을 실행해 ‘주변’에서 ‘페어링’을 선택하거나 ‘기기 추가’에 주소를 입력한 뒤 다른 기기에서 허용하세요. 두 화면에 같은 확인 번호가 표시될 때만 허용하세요.",
      "NAS(SMB, FTP, NFS)도 같은 방법으로 추가합니다. 페어링된 기기와 NAS는 게임 소스이자 출력 위치이며, 그곳의 게임은 편집되지 않습니다.",
      "‘파일 전송’은 이들 사이에서 파일과 폴더를 복사합니다. 대상에 같은 이름이 이미 있으면 확인한 뒤에만 교체됩니다.",
    ],
    helpSecurity: "기기 사이에 보내는 내용은 암호화되지 않으므로 같은 네트워크의 누구나 읽을 수 있습니다. 저장된 NAS 비밀번호는 암호화되지 않은 채 이 기기에 보관됩니다. ‘연결’은 집 네트워크에서만 사용하세요.",
    helpSpeed: "페어링된 기기의 행이나 세부 정보에 있는 ‘속도 테스트’는 그 기기로 128 MiB를 보내고 128 MiB를 받아 오며(디스크에는 아무것도 쓰지 않음), 방향별 네트워크 속도를 보여 줍니다.",
    speedTest: "속도 테스트",
    speedTestWith: (name) => `${name} 속도 테스트`,
    speedTesting: "테스트 중",
    speedResult: (up, down) => `업로드 ${up} MB/s · 다운로드 ${down} MB/s`,
    speedFailed: "속도 테스트 실패:",
    speedNote: "128 MiB를 보내고 128 MiB를 받아 옵니다. 디스크에는 아무것도 쓰지 않습니다.",
    installTitle: "PS5의 Dump Forge",
    ps5Address: "PS5 주소",
    install: "PS5에 설치",
    installHint: "네트워크에서 접근할 수 있는 콘솔의 ELF 로더가 필요합니다(포트 9021).",
    findingRelease: "GitHub에서 릴리스를 찾는 중",
    installing: (v, host) => `${host}에 Dump Forge ${v} 설치 중`,
    installed: (v, host) => `${host}에서 Dump Forge ${v}가 시작되고 있습니다. 실행되면 기기 추가에서 페어링하세요.`,
    noRelease: "GitHub에서 Dump Forge PS5 릴리스를 찾을 수 없습니다. 이 컴퓨터의 인터넷 연결을 확인한 뒤 다시 시도하세요.",
    otherPayload: "다른 ELF 페이로드:",
    sendPayload: "페이로드 보내기",
    pickPayload: "ELF 페이로드 선택",
    sendingPayload: (name, host) => `${name}을(를) ${host}에 보내는 중`,
    sentPayload: (name, size, host) => `${name}(${size})을(를) ${host}에 보냈습니다.`,
    helpInstallTitle: "클릭 한 번으로 PS5에 Dump Forge 설치",
    helpInstallText: "Dump Forge 설치 카드로 콘솔에 페이로드를 보내세요. GitHub에서 맞는 버전(또는 최신 버전)을 내려받아 실행합니다.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "패키지 설치",
    packages: "패키지",
    refresh: "새로 고침",
    scanning: "패키지를 찾는 중",
    none: "이 콘솔에서 패키지를 찾지 못했습니다.",
    noneHint: "콘솔 저장 공간이나 USB 드라이브에 .pkg 파일을 복사한 다음 새로 고침을 누르세요.",
    truncated: "목록이 완전하지 않을 수 있음",
    limits: "검색이 한도 중 하나에 걸려 멈췄습니다. 일부 패키지가 빠져 있을 수 있습니다.",
    cantScan: "검색할 수 없습니다:",
    cantInstall: "설치할 수 없습니다:",
    content: { game: "게임", patch: "패치", additional_content: "추가 콘텐츠", unknown: "알 수 없음" },
    install: "설치",
    installName: (name) => `${name} 설치`,
    started: (name) => `${name}을(를) 설치하는 중입니다. 작업은 아래 목록에 있습니다.`,
    tag: "설치",
    title: "이 패키지를 설치할까요?",
    titleOn: (device) => `이 패키지를 ${device}에 설치할까요?`,
    file: "파일",
    game: "게임",
    titleId: "타이틀 ID",
    version: "버전",
    size: "크기",
    text: "PS5가 Dump Forge에서 패키지를 가져와 설치합니다. 작업이 끝날 때까지 Dump Forge를 실행한 채로 두세요.",
    reinstall: "이 게임이 이미 설치되어 있으면 PS5는 설치된 사본을 삭제한 뒤 이 패키지를 설치합니다.",
    cancel: "취소",
    confirm: "설치",
    installPkg: "pkg 설치",
    installPkgOn: (device) => `${device}에 패키지 설치`,
    pickTitle: "설치할 패키지 선택",
    sent: (device) => `${device}에서 패키지를 설치하는 중입니다. 작업은 아래 목록에 있습니다.`,
  },
  duration: { s: (s) => `${s}초`, ms: (m, s) => `${m}분 ${s}초`, hm: (h, m) => `${h}시간 ${m}분` },
};

export default ko;
