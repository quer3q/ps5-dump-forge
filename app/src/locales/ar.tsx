// Arabic (Modern Standard Arabic). Typed as `Messages` (en.tsx's shape): tsc fails on a missing key.
// Terms: تتبّع / بيانات التتبّع (trace / traces), حزم / فك الحزم (pack / unpack),
// ترقيع / إلغاء الترقيع (patch / unpatch), بيئة التشغيل (runtime).

import { dotted, num, plural } from "../i18n";
import type { Messages } from "./en";

/** A runtime value (path, file or folder name, URL, backend text) isolated left-to-right (LRI…PDI)
 * from the Arabic around it: a path stays in its order even when a folder name is Arabic. */
const iso = (v: string) => `\u2066${v}\u2069`;

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "مساوٍ لحجم اللعبة";

/** A noun's forms after a count: one and two stand alone (ملف واحد, ملفان), 3–10 take the
 * plural, 11–99 the singular accusative, 100 and up the singular. */
type Noun = { one: string; two: string; few: string; many: string; other: string };

/** A count and its noun as one phrase, in every Arabic plural category. */
const count = (n: number, w: Noun): string =>
  plural(n, {
    zero: `${num(n)} ${w.few}`,
    one: w.one,
    two: w.two,
    few: `${num(n)} ${w.few}`,
    many: `${num(n)} ${w.many}`,
    other: `${num(n)} ${w.other}`,
  });

const FILES: Noun = { one: "ملف واحد", two: "ملفان", few: "ملفات", many: "ملفًا", other: "ملف" };
/** After a preposition or as an object. */
const FILES_GEN: Noun = { ...FILES, two: "ملفين" };
const CHECKS: Noun = { one: "اختبار واحد", two: "اختباران", few: "اختبارات", many: "اختبارًا", other: "اختبار" };
const FOLDERS: Noun = { one: "مجلد واحد", two: "مجلدان", few: "مجلدات", many: "مجلدًا", other: "مجلد" };
const ITEMS: Noun = { one: "عنصر واحد", two: "عنصران", few: "عناصر", many: "عنصرًا", other: "عنصر" };
const DEVICES: Noun = { one: "جهاز واحد", two: "جهازان", few: "أجهزة", many: "جهازًا", other: "جهاز" };
/** After منذ ("ago"), in the genitive. */
const MINUTES: Noun = { one: "دقيقة", two: "دقيقتين", few: "دقائق", many: "دقيقةً", other: "دقيقة" };
const HOURS: Noun = { one: "ساعة", two: "ساعتين", few: "ساعات", many: "ساعةً", other: "ساعة" };
const DAYS: Noun = { one: "يوم", two: "يومين", few: "أيام", many: "يومًا", other: "يوم" };
const RESULTS: Noun = { one: "نتيجة واحدة", two: "نتيجتان", few: "نتائج", many: "نتيجةً", other: "نتيجة" };
const LINES: Noun = { one: "سطر واحد", two: "سطران", few: "أسطر", many: "سطرًا", other: "سطر" };
const BACKPORT_LIBS: Noun = {
  one: "مكتبة باكبورت واحدة",
  two: "مكتبتَي باكبورت",
  few: "مكتبات باكبورت",
  many: "مكتبة باكبورت",
  other: "مكتبة باكبورت",
};

const ar: Messages = {
  // App.tsx
  app: {
    screens: "الأقسام",
    active: (n) => `، النشطة: ${num(n)}`,
    edit: "تحرير",
    storage: "التخزين",
    connect: "الاتصال",
    disconnected: "غير متصل",
    connected: (n: number) => count(n, DEVICES),
    about: "حول",
    offline: "غير متصل، جارٍ إعادة المحاولة…",
    stop: "إيقاف",
    stillRunning: (n) =>
      plural(n, {
        zero: `لا تزال ${num(n)} مهام قيد التشغيل`,
        one: "لا تزال مهمة قيد التشغيل",
        two: "لا تزال مهمتان قيد التشغيل",
        few: `لا تزال ${num(n)} مهام قيد التشغيل`,
        many: `لا تزال ${num(n)} مهمةً قيد التشغيل`,
        other: `لا تزال ${num(n)} مهمة قيد التشغيل`,
      }),
    job: (id) => `المهمة ${id}`,
    quitNote: (web, n) =>
      n === 1
        ? web
          ? "سيؤدي إيقاف Dump Forge إلى إيقاف المهمة وحذف ناتجها غير المكتمل."
          : "سيؤدي الخروج إلى إيقاف المهمة وحذف ناتجها غير المكتمل."
        : web
          ? "سيؤدي إيقاف Dump Forge إلى إيقاف المهام وحذف نواتجها غير المكتملة."
          : "سيؤدي الخروج إلى إيقاف المهام وحذف نواتجها غير المكتملة.",
    keepRunning: "متابعة التشغيل",
    stopAndQuit: (n) => (n === 1 ? "إيقاف المهمة والخروج" : "إيقاف المهام والخروج"),
    stoppingJobs: "جارٍ إيقاف المهام والتنظيف…",
    stoppingForge: "جارٍ إيقاف Dump Forge…",
    quitting: "جارٍ إنهاء PS5 Dump Forge…",
    couldntStop: "تعذّر إيقاف Dump Forge",
    close: "إغلاق",
    tryAgain: "إعادة المحاولة",
    stopped: "توقّف Dump Forge",
    startAgain: (ps5) => (ps5 ? "للمتابعة، افتح لوحة Dump Forge على PS5 مرة أخرى." : "للمتابعة، شغّل أمر ⁦ps5-dump-forge serve⁩ نفسه مرة أخرى."),
    confirmStop: (web: boolean) => (web ? "إيقاف Dump Forge؟" : "إنهاء PS5 Dump Forge؟"),
    stopNote: (web, ps5) => (!web ? "لا توجد مهمة قيد التشغيل." : ps5 ? "لتشغيله مرة أخرى، افتح لوحة Dump Forge على PS5." : "لتشغيله مرة أخرى، شغّل أمر ⁦ps5-dump-forge serve⁩ نفسه من جديد."),
    peerBusy: "جهاز مقترن يقرأ ملفات أو يكتبها على هذا الكمبيوتر. الخروج يوقف عملية النقل الخاصة به.",
    quit: "إنهاء",
    github: (name) => `${iso(name)} على GitHub`,
    queued: "في الانتظار",
    progress: (percent, left) => `اكتمل ${percent}%${left ? `، المتبقي ${left}` : ""}`,
  },
  // build/ (Build's convert settings and checks)
  convert: {
    targetFormat: "الصيغة",
    alreadyFolder: "المصدر مجلد بالفعل",
    patchBlocked: "لا يمكن البناء: هذه حزمة تصحيح لا تحتوي إلا على ما غيّره التحديث. افتحها في «تحرير» لاستخراج ملفاتها.",
    openInEdit: "فتح في «تحرير»",
    noContentId: "لا يمكن البناء بصيغة FPKG: لا يحتوي ⁦param.json⁩ لهذه اللعبة على Content ID، وهو ما تحتاجه الحزمة.",
    blocked: (first, more) =>
      `لا يمكن البناء: ${iso(first)}${more > 0 ? ` (بالإضافة إلى ${count(more, { one: "ملاحظة أخرى", two: "ملاحظتين أخريين", few: "ملاحظات أخرى", many: "ملاحظةً أخرى", other: "ملاحظة أخرى" })}، راجع «المصدر»)` : ""}.`,
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "لا يطابق ⁦ampr_emu.index⁩ ملفات هذه اللعبة" : state === "damaged" ? "تتعذّر قراءة ⁦ampr_emu.index⁩" : "لا يوجد ⁦ampr_emu.index⁩ في هذه اللعبة"}: سيكتب التحويل فهرسًا جديدًا.`,
    removeBackport: "إزالة الباكبورت",
    removeHint: (libs, emulators, other) =>
      `يستبعد من ⁦fakelib/⁩ ${count(libs, { one: "مكتبة باكبورت واحدة", two: "مكتبتَي باكبورت", few: "مكتبات باكبورت", many: "مكتبة باكبورت", other: "مكتبة باكبورت" })}` +
      (emulators.length === 0
        ? other
          ? "؛ وتبقى مكتبات homebrew الأخرى."
          : "."
        : `؛ وتبقى المحاكيات (${emulators.join("، ")})${other ? " ومكتبات homebrew الأخرى" : ""}.`),
    cantRemove: (why) => `تعذّرت إزالة الباكبورت: ${iso(why)}`,
    compare: "مقارنة الصيغ",
    build: "بناء",
    likelyFail: (fmt) => (
      <>
        من المرجّح أن يفشل البناء بصيغة <b>{fmt}</b>: فهي ترفض أسماء ملفات مدرجة في «المصدر».
      </>
    ),
    collides: "هناك مهمة تكتب هذا الملف بالفعل: ستفشل إعادة «بناء». اختر اسمًا آخر.",
    already: (fmt) => (
      <>
        يجري بالفعل بناء هذه اللعبة بصيغة <b>{fmt}</b>؛ إعادة «بناء» تُنشئ نسخة ثانية.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "جيد", ok: "حل وسط", bad: "عيب" },
    columns: ["السرعة على PS5", "الحجم على القرص"],
    rows: {
      pkg: {
        tagline: "الأفضل، إن سمح البرنامج الثابت",
        cells: ["أصلية، بالسرعة الكاملة", "أصغر بنسبة 30–60%، حسب اللعبة"],
      },
      zstd: { tagline: "مع ampr-emu-ng", cells: ["تحميل أسرع", "حتى نصف حجم اللعبة"] },
      lz4: {
        tagline: "مع ampr-emu-ng",
        cells: [
          "تحميل أسرع بنسبة 30–40%، حسب أرقام المطوّرين الأصليين",
          "نحو نصف حجم اللعبة، حسب أرقام المطوّرين الأصليين",
        ],
      },
      ffpkg: { tagline: "مستحسن لـ ShadowMountPlus", cells: ["السرعة الكاملة للصورة", FLAT] },
      exfat: { tagline: "للتوافق", cells: ["السرعة الكاملة للصورة", FLAT] },
      ffpfs: { tagline: "PFS غير مضغوط", cells: ["السرعة الكاملة للصورة", FLAT] },
      ffpfsc: {
        tagline: "الأصغر",
        cells: ["150–250 ⁦MB/s⁩، وقد يحدث تقطّع", "حسب مستوى الضغط، عادةً أصغر بنسبة 30–60%"],
      },
      folder: { tagline: "ملفات اللعبة كما هي", cells: ["السرعة الكاملة لوحدة التخزين", FLAT] },
    },
    compare: "مقارنة الصيغ",
    compareLabel: "مقارنة الصيغ",
    format: "الصيغة",
    credits: "شكر وتقدير",
    upstream: (app: string) =>
      `أرقام المطوّرين الأصليين: ما يذكره مؤلفو ampr_emu و Lazy_AMPR، ولم يقِسها ${app}؛ وهي تختلف من لعبة إلى أخرى.`,
    bundled: "⁦ampr_emu⁩ من تطوير drakmor (⁦GPL-3.0⁩)، وقد تفرّع عنه ⁦ampr-emu-ng⁩. لم يعد Forge يضمّنه: بل يستبدل ⁦ampr_emu⁩ الخاص باللعبة بـ ⁦ampr-emu-ng⁩ ما لم تُبقِ عليه «الإعدادات». الشيفرة المصدرية:",
    lazy: "تعتمد استراتيجية الحزم على Lazy_AMPR من تطوير Nazky:",
    ng: "تعمل حزم ZSTD و LZ4 على ⁦ampr-emu-ng⁩ (⁦GPL-3.0-or-later⁩)، بيئة تشغيل AMPR الوحيدة المضمّنة، دون تعديل. المصدر:",
    logo: "الشعار من تصميم Xxcco:",
    unrar:
      "تُقرأ أرشيفات ⁦.rar⁩ بمكتبة UnRAR المضمّنة من تطوير Alexander Roshal (للاستخراج فقط، بموجب ترخيص UnRAR).",
    language: "اللغة",
    system: "لغة النظام",
    notifications: "الإشعار عند اكتمال المهمة",
    notificationsHint: "يظهر على الجهاز عند انتهاء مهمة بناء أو حزم أو نقل ملفات.",
    notificationsFailed: "تعذّر تغيير الإشعارات.",
  },
  // Edit.tsx
  edit: {
    details: "التفاصيل",
    deleting: "جارٍ حذف المصدر",
    delete: "حذف المصدر",
    deleted: "تم الحذف:",
    filter: "تصفية حسب المسار",
    filterLabel: "تصفية الملفات حسب المسار",
    firstOf: (max, n) => `أول ${num(max)} من ${num(n)}`,
    fileCount: (n) => count(n, FILES),
    noMatch: (f) => `لا توجد ملفات تطابق «${iso(f)}».`,
    noListing: "عرض قائمة ملفات الأرشيف غير مدعوم.",
    noParam: "لا يوجد ⁦sce_sys/param.json⁩.",
    reader: "القارئ",
    leftovers: "ملفات ومجلدات متبقية",
    found: (n) =>
      plural(n, {
        zero: `عُثر على ${num(n)}`,
        one: "عُثر على عنصر واحد",
        two: "عُثر على عنصرين",
        few: `عُثر على ${num(n)} عناصر`,
        many: `عُثر على ${num(n)} عنصرًا`,
        other: `عُثر على ${num(n)} عنصر`,
      }),
    leftoversNote: (app) =>
      `نواتج غير مكتملة، عادةً من مهام توقفت قبل انتهائها. لا تحذف إلا ما تعرفه منها، وفقط حين لا تعمل أي مهمة؛ لا يحذفها ${app} من تلقاء نفسه أبدًا.`,
    checked: (n) => `المجلدات التي فُحصت (${num(n)})`,
    facts: "معلومات اللعبة",
    explorer: "ملفات اللعبة",
    howTitle: "كيف يعمل هذا",
    how: (app) => [
      "افتح **مجلد لعبة** أو **صورتها**، ثم عدّلها كما تعدّل مجلدًا على ذاكرة USB: **أضف الملفات واستبدلها** (اسحبها إلى هنا أو استخدم «إضافة»)، **وأعد التسمية واحذف**.",
      `لا يتغير شيء **حتى تضغط «تطبيق»**: تتراكم تغييراتك هنا، ثم يكتبها ${app} مباشرةً في الصورة أو المجلد دفعة واحدة ويتحقق من النتيجة. **هذا ليس قرصًا مُركّبًا**: ${app} يسجّل تغييراتك ويطبّقها بنفسه.`,
      `حين تتسع الصورة للتغييرات **تُكتب فيها مباشرةً**؛ وحين لا تتسع (لا مساحة متبقية بداخلها، أو صيغة لا تقبلها)، يبني ${app} بجانبها صورة جديدة بتغييراتك ويضعها مكانها بعد التأكد من سلامتها. وفي الحالتين، إن فشل شيء تبقى الصورة كما كانت.`,
      "يمكن تصفّح الأرشيفات لكن لا يمكن تحريرها.",
    ],
    location: "الموقع داخل اللعبة",
    root: "اللعبة",
    up: "أعلى",
    upTip: "الصعود مجلدًا واحدًا",
    name: "الاسم",
    size: "الحجم",
    actions: "الإجراءات",
    items: (n) => count(n, ITEMS),
    inFolder: (dirs, files) => `${count(dirs, FOLDERS)}، ${count(files, FILES)}`,
    matches: (n) => count(n, RESULTS),
    emptyFolder: "هذا المجلد فارغ.",
    open: (name) => `فتح ${iso(name)}`,
    archiveNote: "يمكن تصفّح الأرشيفات لكن لا يمكن تحريرها. ابنِ منه مجلدًا أو صورة أولًا لتعديله.",
    rebuildTimePkg: (t) => `تعيد إعادة البناء ضغط اللعبة كلها: نحو ${t} بسرعة ~26 MB/s على PS5 (أسرع على الكمبيوتر). عدّل نسخة غير محزومة بدلًا من ذلك لتتجنّب ذلك.`,
    rebuildTimePs5: (t) => `تعيد إعادة البناء كتابة الصورة كلها (نحو ${t} على جهاز PS5 هذا).`,
    rebuildTimeHost: (t) => `تعيد إعادة البناء كتابة الصورة كلها (نحو ${t} على هذا الكمبيوتر).`,
    rebuildTimeAny: (t) => `تعيد إعادة البناء كتابة الصورة كلها (نحو ${t} على PS5).`,
    nasNote: "يمكن تصفح الألعاب الموجودة على NAS واستخراجها، لكن لا يمكن تعديلها. لتغييرها، ابنِها أو استخرجها إلى هذا الجهاز أولًا.",
    patchNote: "هذه حزمة تصحيح: لا تحتوي إلا على ما غيّره التحديث. تظهر الملفات التي تحتويها كاملة؛ يمكن تصفحها واستخراجها، لكن لا يمكن تعديلها.",
    peerNote: "يمكن تصفح الألعاب الموجودة على Forge آخر واستخراجها، لكن لا يمكن تعديلها. لتغييرها، ابنِها أو استخرجها إلى هذا الجهاز أولًا.",
    addFiles: "إضافة ملفات",
    addFolder: "إضافة مجلد",
    pickAddFiles: "اختر ملفات لإضافتها",
    pickAddFolder: "اختر مجلدًا لإضافته",
    peerRoot: "لا يمكن إضافة جهاز كامل: اختر مجلدًا داخله.",
    renameItem: (name) => `إعادة تسمية ${iso(name)}`,
    removeItem: (name) => `حذف ${iso(name)}`,
    renameTip: "إعادة التسمية (F2)",
    removeTip: "حذف (مفتاح Delete)",
    removeTipMac: "حذف (fn+Delete)",
    extractItem: (name: string) => `استخراج ${iso(name)}`,
    extractTip: "نسخه إلى مجلد تختاره",
    extractFolder: "استخراج المجلد",
    extractFolderTip: "نسخ المجلد المعروض إلى مجلد تختاره",
    extractGameTip: "نسخ اللعبة كاملة إلى مجلد تختاره",
    extractNotOnDisk: "هذا المجلد موجود في تغييراتك المعلّقة فقط: طبّقها لاستخراجه.",
    pickExtract: "اختر مكان الاستخراج",
    extractPending: "يستخرج الملفات كما هي الآن، دون تغييراتك المعلّقة.",
    nameEmpty: "اكتب اسمًا.",
    nameBad: `لا يمكن أن يحتوي الاسم على ${iso("/")} أو ${iso("\\")}، ولا أن يكون ${iso(".")} أو ${iso("..")}`,
    nameTaken: (name) => `${iso(name)} موجود هنا بالفعل.`,
    markNew: "جديد",
    markReplaced: "مُستبدَل",
    markRenamed: "أُعيدت تسميته",
    wasAt: (path) => `كان في ${iso(path)}`,
    sizeLater: "أكثر مما أمكن سرده: يُقرأ الباقي عند تطبيق التغييرات",
    dropInto: (folder) => `أفلِت للإضافة إلى ${iso(folder)}`,
    droppedGone: (path) => `${iso(path)}: الملف المسحوب لم يعد موجودًا`,
    pending: (n) =>
      plural(n, {
        zero: `${num(n)} تغييرات لم تُطبَّق بعد`,
        one: "تغيير واحد لم يُطبَّق بعد",
        two: "تغييران لم يُطبَّقا بعد",
        few: `${num(n)} تغييرات لم تُطبَّق بعد`,
        many: `${num(n)} تغييرًا لم يُطبَّق بعد`,
        other: `${num(n)} تغيير لم يُطبَّق بعد`,
      }),
    undo: "تراجع",
    undoTip: "التراجع عن آخر إجراء بكل تغييراته",
    discard: "تجاهل الكل",
    discardTip: "نسيان كل تغيير لم يُطبَّق بعد",
    discardTitle: (n) =>
      plural(n, { one: "تجاهل تغيير واحد؟", two: "تجاهل التغييرين كليهما؟", other: `تجاهل كل التغييرات (${num(n)})؟` }),
    discardText: "ستُنسى، ولن يتغير شيء في اللعبة. لا يمكن التراجع عن هذا.",
    discardNo: "لا",
    changesList: "التغييرات بالترتيب",
    willNotApply: (n) =>
      plural(n, {
        zero: `${num(n)} تغييرات لن تُطبَّق كما هي. تراجع عنها أو تجاهل الكل.`,
        one: "تغيير واحد لن يُطبَّق كما هو. تراجع عنه أو تجاهل الكل.",
        two: "تغييران لن يُطبَّقا كما هما. تراجع عنهما أو تجاهل الكل.",
        few: `${num(n)} تغييرات لن تُطبَّق كما هي. تراجع عنها أو تجاهل الكل.`,
        many: `${num(n)} تغييرًا لن تُطبَّق كما هي. تراجع عنها أو تجاهل الكل.`,
        other: `${num(n)} تغيير لن تُطبَّق كما هي. تراجع عنها أو تجاهل الكل.`,
      }),
    opAdd: (to) => <>إضافة {to}</>,
    opAddFolder: (to) => <>إضافة المجلد {to}</>,
    opDelete: (path) => <>حذف {path}</>,
    opRename: (from, to) => <>إعادة تسمية {from} إلى {to}</>,
    opMkdir: (path) => <>مجلد جديد {path}</>,
    opFrom: (src) => <>من {src}</>,
    whyInvalid: (path) => <>{path} ليس مسارًا صالحًا في اللعبة.</>,
    whyMissing: (path) => <>{path} غير موجود في اللعبة.</>,
    whyExists: (path) => <>{path} موجود بالفعل.</>,
    whyFile: (path) => <>{path} ملف.</>,
    whyFolder: (path) => <>{path} مجلد.</>,
    whyInto: (path) => <>لا يمكن نقل {path} إلى داخل نفسه.</>,
    conflictTitle: (n) =>
      plural(n, {
        zero: `${num(n)} عناصر موجودة هنا بالفعل`,
        one: "عنصر واحد موجود هنا بالفعل",
        two: "عنصران موجودان هنا بالفعل",
        few: `${num(n)} عناصر موجودة هنا بالفعل`,
        many: `${num(n)} عنصرًا موجودًا هنا بالفعل`,
        other: `${num(n)} عنصر موجود هنا بالفعل`,
      }),
    conflictIn: (folder) => <>في {folder}:</>,
    conflictText:
      "يضع «استبدال» العناصر الجديدة مكانها: بعد تطبيق التغييرات يختفي المحتوى القديم نهائيًا، وقد يمنع ملف تالف اللعبة من الإقلاع. المجلد المضاف فوق مجلد يُدمج معه، وتُستبدل ملفاته ذات الأسماء نفسها.",
    replace: "استبدال",
    skip: "تخطي",
    cancel: "إلغاء",
    modeLabel: "طريقة التطبيق",
    checking: "جارٍ التحقق من طريقة تطبيق التغييرات",
    nothingToChange: "لا شيء لتغييره: تبقى اللعبة كما هي.",
    checkFailed: "تعذّر التحقق من التغييرات:",
    checkAgain: "التحقق مجددًا",
    inPlace: "في مكانها",
    rebuild: "إعادة بناء",
    rebuildNeeded: "تلزم إعادة البناء",
    cantApply: "لا يمكن التطبيق",
    changedDirectly: "تعديل مباشر",
    folderText: "تُكتب مباشرةً في المجلد.",
    inPlaceText: "تُكتب في الصورة.",
    inPlaceNeeds: (need, free) => `تُكتب في الصورة؛ تحتاج إلى ${iso(need)} من ${iso(free)} متاحة فيها.`,
    noRoom: (need, free) => `تحتاج التغييرات إلى ${iso(need)}، والمساحة المتاحة في الصورة ${iso(free)}.`,
    alwaysRebuilt: "يُعاد بناء صور ffpfsc و FPKG دائمًا.",
    rebuildText: "تُكتب صورة جديدة بجانبها وتحل محلها بعد التحقق منها.",
    rebuildNeeds: (need, free) =>
      `تُكتب صورة جديدة بجانبها (تلزم مساحة متاحة نحو ${iso(need)}، والمتاح ${iso(free)}) وتحل محلها بعد التحقق منها.`,
    uploadLater: (n) =>
      plural(n, {
        zero: `تُفحص ${num(n)} ملفات من هذا المتصفح بعد رفعها، حين تضغط «تطبيق».`,
        one: "يُفحص ملف واحد من هذا المتصفح بعد رفعه، حين تضغط «تطبيق».",
        two: "يُفحص ملفان من هذا المتصفح بعد رفعهما، حين تضغط «تطبيق».",
        few: `تُفحص ${num(n)} ملفات من هذا المتصفح بعد رفعها، حين تضغط «تطبيق».`,
        many: `يُفحص ${num(n)} ملفًا من هذا المتصفح بعد رفعها، حين تضغط «تطبيق».`,
        other: `يُفحص ${num(n)} ملف من هذا المتصفح بعد رفعها، حين تضغط «تطبيق».`,
      }),
    decidedOnApply: (n) =>
      plural(n, {
        one: "تعتمد تغييرات أخرى على الملف القادم من هذا المتصفح، لذا تتحدد طريقة تطبيقها بعد رفعه، حين تضغط «تطبيق».",
        two: "تعتمد تغييرات أخرى على الملفين القادمين من هذا المتصفح، لذا تتحدد طريقة تطبيقها بعد رفعهما، حين تضغط «تطبيق».",
        other: `تعتمد تغييرات أخرى على ${count(n, FILES_GEN)} من هذا المتصفح، لذا تتحدد طريقة تطبيقها بعد رفعها، حين تضغط «تطبيق».`,
      }),
    sourceLocked: "يجري تطبيق تغييرات على هذه اللعبة. اختر مصدرًا آخر بعد انتهاء المهمة.",
    notOpened: (path) => <>لم يُفتح {path}: يجري تطبيق تغييرات على هذه اللعبة. افتحه مجددًا بعد انتهاء المهمة.</>,
    apply: "تطبيق",
    applyTip: "تطبيق كل التغييرات المدرجة هنا",
    applyTitle: (n: number) =>
      `تطبيق ${count(n, { one: "تغيير واحد", two: "تغييرين", few: "تغييرات", many: "تغييرًا", other: "تغيير" })}؟`,
    applyInPlace: (name) => <>تُكتب مباشرةً في {name}. لا يمكن التراجع بعد إتمام ذلك.</>,
    applyRebuild: (name) => (
      <>تُكتب صورة جديدة بتغييراتك بجانب {name}، ويُتحقق منها، ثم توضع مكانها. لا يمكن التراجع بعد أن تحل محل الصورة.</>
    ),
    closeImage: "إن كانت اللعبة تعمل من هذه الصورة، فأغلقها أولًا.",
    closeFolder: "إن كانت اللعبة تعمل من هذا المجلد، فأغلقها أولًا.",
    applyRestores: "انقطع تحرير سابق لهذه الصورة: تُعاد الصورة أولًا إلى ما كانت عليه.",
    uploadingFile: (i: number, n: number) => `جارٍ رفع الملف ${num(i)} من ${num(n)}`,
    uploading: (i: number, n: number, sent: string, total: string) =>
      `جارٍ رفع الملف ${num(i)} من ${num(n)}: ${iso(sent)} من ${iso(total)}`,
    uploadLabel: "الرفع",
    uploadCancelled: "أُلغي الرفع. لم يتغير شيء في اللعبة.",
    applyRunning: "جارٍ تطبيق التغييرات. ينتظر التحرير حتى تنتهي المهمة (انظر «قائمة الانتظار»).",
    applyFailed: "لم تُطبَّق التغييرات:",
    applyRefused: "لا يمكن تطبيق التغييرات بهذه الطريقة:",
    interrupted: "انقطع تحرير سابق لهذه الصورة. تُعاد الصورة إلى ما كانت عليه قبل فحص تغييراتك التالية أو تطبيقها.",
  },
  // JobList.tsx
  jobs: {
    jobs: "المهام",
    pending: (n) =>
      plural(n, {
        zero: `${num(n)} قيد التشغيل أو في الانتظار`,
        one: "مهمة واحدة قيد التشغيل أو في الانتظار",
        two: "مهمتان قيد التشغيل أو في الانتظار",
        few: `${num(n)} مهام قيد التشغيل أو في الانتظار`,
        many: `${num(n)} مهمةً قيد التشغيل أو في الانتظار`,
        other: `${num(n)} مهمة قيد التشغيل أو في الانتظار`,
      }),
    allFinished: "اكتملت كلها",
    stages: {
      scan: "قراءة المصدر",
      preflight: "التحقق",
      check: "التحقق",
      plan: "التخطيط",
      compress: "الضغط",
      write: "الكتابة",
      measure: "حساب أحجام الحزم",
      pack: "الحزم",
      verify: "التحقق من الناتج",
      finalize: "الإنهاء",
      install: "التثبيت",
    },
    steps: {
      scan: "القراءة",
      preflight: "التحقق",
      check: "التحقق",
      plan: "التخطيط",
      compress: "الضغط",
      write: "الكتابة",
      measure: "القياس",
      pack: "الحزم",
      verify: "التدقيق",
      finalize: "الإنهاء",
      install: "التثبيت",
    },
    stepsLabel: "الخطوات",
    stepDone: (s) => `${s}: مكتملة`,
    stepFailed: (s) => `${s}: فشلت`,
    stepLater: (s) => `${s}: لم تبدأ بعد`,
    packingWriting: "الحزم والكتابة",
    rate: { read: "قراءة", compress: "ضغط", pack: "حزم", write: "كتابة" },
    showPath: "إظهار المسار",
    showFinder: "إظهار في Finder",
    showExplorer: "إظهار في Explorer",
    showFolder: "إظهار في المجلد",
    packTag: {
      lz4Pack: (level: string) => `LZ4: حزم · ${level}`,
      lz4Unpack: "LZ4: فك الحزم",
      zstdPack: (level: string) => `ZSTD: حزم · ${level}`,
      level: (n: number) => `المستوى ${num(n)}`,
      zstdUnpack: "ZSTD: فك الحزم",
      withRules: (tag) => `${tag} · قواعد مخصّصة`,
    },
    edit: "تحرير",
    editRebuild: "تحرير · إعادة بناء",
    transfer: "نقل",
    extract: "استخراج",
    job: (id) => `المهمة ${id}`,
    done: "اكتملت",
    cancelled: "أُلغيت",
    failed: "فشلت",
    cancelling: "جارٍ الإلغاء…",
    queued: "في الانتظار",
    dismiss: "إخفاء",
    retry: "إعادة المحاولة",
    retrying: "جارٍ البدء",
    stopJob: "إيقاف المهمة؟",
    cancel: "إلغاء",
    stopHint: "اضغط «إيقاف المهمة؟» مرة أخرى خلال 4 ثوانٍ لإيقافها.",
    remoteLabel: (name, device) => `${iso(name)}، مهمة على ${iso(device)}`,
    remoteTitle: (device) => `تعمل على ${iso(device)} وتستخدم ملفات هذا الجهاز: يمكنك هنا إلغاؤها فقط`,
    from: "من ",
    left: (d) => `المتبقي ${d}`,
    inStep: (time) => `${time} في هذه الخطوة`,
    outputPath: "مسار الناتج",
    sourceDeleted: (v) => `حُذف المصدر: ${iso(v)}`,
    sourceKept: (v) => `أُبقي المصدر: ${iso(v)}`,
    sourcePartlyDeleted: (v) => `حُذف المصدر جزئيًا: ${iso(v)}`,
    checks: "الاختبارات",
    verifiedPlain: (n) =>
      plural(n, {
        zero: `تم التحقق: نجحت ${num(n)} اختبارات`,
        one: "تم التحقق: نجح اختبار واحد",
        two: "تم التحقق: نجح اختباران",
        few: `تم التحقق: نجحت ${num(n)} اختبارات`,
        many: `تم التحقق: نجح ${num(n)} اختبارًا`,
        other: `تم التحقق: نجح ${num(n)} اختبار`,
      }),
    verifiedFull: (n) => `نجح التحقق الكامل: ${count(n, CHECKS)}`,
    verifiedFast: (n) => `نجح التحقق السريع: ${count(n, CHECKS)}`,
    coverage: (checked, total, n) =>
      `فُحص ${checked} من ${total} في ${count(n, { one: "عيّنة واحدة", two: "عيّنتين", few: "عيّنات", many: "عيّنةً", other: "عيّنة" })}`,
    cantBuild: "تعذّر بناء هذه اللعبة:",
    cantEdit: "تعذّر تطبيق التغييرات:",
    cantExtract: "تعذّر الاستخراج:",
    cantTransfer: "تعذّر النقل:",
    cantBuildAs: (f) => (
      <>
        تعذّر بناء هذه اللعبة بصيغة <b>{f}</b>:
      </>
    ),
    log: (n) => `السجل (${num(n)})`,
    logLabel: "السجل",
  },
  // Storage text shared by Files.tsx, Packages.tsx, Edit.tsx and common.tsx (FreeSpace)
  library: {
    cantDelete: "تعذّر الحذف:",
    deleted: (path) => `تم حذف ${iso(path)}`,
    storageTip: "المساحة الحرة في التخزين الداخلي لجهاز PS5 (⁦/data⁩)",
    free: (free, total) => (
      <>
        متاح <b>{iso(free)}</b> من {iso(total)}
      </>
    ),
    cards: "عرض البطاقات",
    list: "عرض القائمة",
    remove: (n) => `حذف ${iso(n)}`,
    deleting: (n) => `جارٍ حذف ${iso(n)}`,
    showAs: "طريقة العرض",
  },
  // Storage.tsx, Files.tsx: the Storage tab (File explorer | Package installer)
  storage: {
    views: "طريقة عرض التخزين",
    files: "مستكشف الملفات",
    title: "مستكشف الملفات",
    roots: "محركات الأقراص",
    location: "الموقع",
    folders: "المجلدات",
    up: "أعلى",
    upTip: "الصعود مجلدًا واحدًا",
    refresh: "تحديث",
    filter: "تصفية حسب الاسم",
    filterLabel: "تصفية هذا المجلد حسب الاسم",
    inFolder: (dirs, files) => `${count(dirs, FOLDERS)}، ${count(files, FILES)}`,
    matches: (n) => count(n, RESULTS),
    firstOf: (max, n) => `أول ${num(max)} من ${num(n)}`,
    truncated: "يحتوي هذا المجلد على أكثر من 10,000 عنصر: لا يُعرض منها إلا أول 10,000.",
    name: "الاسم",
    size: "الحجم",
    emptyFolder: "هذا المجلد فارغ.",
    noMatch: (f) => `لا شيء هنا يطابق «${iso(f)}».`,
    noRoots: "لا توجد محركات أقراص متاحة للتصفّح.",
    cantList: "تعذّر فتح هذا المجلد:",
    link: "رابط",
    linkTip: "رابط: يمكن فتحه، لكن لا يمكن إعادة تسميته أو نقله أو حذفه هنا",
    mounted: "مُركَّب",
    mountedTip: `مُركَّب بواسطة ${iso("ShadowMount+")}: أوقف اللعبة وأوقف تشغيل ${iso("ShadowMount+")} لحذفه أو إعادة تسميته أو نقله`,
    mountedNote: `الصور التي تحمل وسم «مُركَّب» يستخدمها ${iso("ShadowMount+")}: أوقف اللعبة وأوقف تشغيل ${iso("ShadowMount+")} لحذفها أو إعادة تسميتها أو نقلها.`,
    open: (n) => `فتح ${iso(n)}`,
    more: (n) => `إجراءات أخرى لـ ${iso(n)}`,
    rename: (n) => `إعادة تسمية ${iso(n)}`,
    renameTip: "إعادة التسمية (F2)",
    move: (n) => `نقل ${iso(n)}`,
    moveTip: "النقل إلى مجلد آخر",
    moveTitle: (n) => `نقل ${iso(n)} إلى`,
    remove: (n) => `حذف ${iso(n)}`,
    removeTip: "حذف (مفتاح Delete)",
    removeTipMac: "حذف (fn+Delete)",
    cantDelete: "تعذّر الحذف:",
    cantRename: "تعذّرت إعادة التسمية:",
    cantMove: "تعذّر النقل:",
    sameFolder: "إنه موجود في ذلك المجلد بالفعل.",
    deleted: (path) => `تم حذف ${iso(path)}`,
    renamed: (path) => `تمت إعادة التسمية إلى ${iso(path)}`,
    moved: (path) => `تم النقل إلى ${iso(path)}`,
    devices: "الأجهزة",
    thisDevice: "هذا الجهاز",
    otherDevice: "النقل إلى مجلد آخر يبقى على جهاز واحد: للنسخ بين الأجهزة استخدم شاشة «النقل».",
    loadingDevices: "جارٍ تحميل الأجهزة",
    noDevice: "لا يوجد جهاز متصل",
    noDeviceHint: "اقرن جهاز PS5 أو كمبيوتر، أو أضف NAS، لإدارة ملفاته هنا.",
    openConnect: "فتح «الاتصال»",
    dropMove: (folder) => `أفلِت للنقل إلى ${iso(folder)}`,
    cantCopy: "تعذّر النسخ:",
    copyNoFolder: "افتح مجلدًا على ذلك الجهاز أولًا.",
    copyOne: (name: string, to: string) => `جارٍ نسخ ${iso(name)} إلى ${iso(to)}. يظهر النقل في «قائمة الانتظار».`,
    copyMany: (n: number, to: string) =>
      `جارٍ نسخ ${count(n, { ...ITEMS, two: "عنصرين" })} إلى ${iso(to)}. يظهر النقل في «قائمة الانتظار».`,
  },
  // build/ (Build's pack settings and checks), JobList.tsx's pack tags
  pack: {
    help: "كيف يعمل الحزم",
    patch: "لا يمكن حزم حزمة تصحيح.",
    additionalContent: "لا يمكن حزم المحتوى الإضافي (بلا ⁦eboot.bin⁩).",
    levels: { fast: "سريع", medium: "متوسط", max: "أقصى" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "مستحسن. ZSTD 5: سريع، وحجم جيد." },
        medium: { text: "ZSTD 9: أصغر، وأبطأ." },
        max: { text: "ZSTD 19: الأصغر؛ أبطأ بكثير، وذاكرة أكثر." },
      },
      lz4: {
        fast: { tone: "good", text: "مستحسن. LZ4 الافتراضي: الأسرع." },
        medium: { text: "LZ4 HC 9: أصغر، وأبطأ." },
        max: { text: "LZ4 HC 12: الأصغر، والأبطأ." },
      },
    },
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "⁦ampr_emu.index⁩ قديم" : state === "damaged" ? "⁦ampr_emu.index⁩ تالف" : "لا يوجد ⁦ampr_emu.index⁩"}: سيكتب الحزم فهرسًا جديدًا.`,
    rules: {
      title: "قواعد مخصّصة (TOML)",
      titleWith: (name) => `قواعد مخصّصة (TOML): ${iso(name)}`,
      info: "ملف تعريف لاستراتيجية الحزم بصيغة ⁦ampr_pack.py⁩ من ⁦ampr_emu⁩ و⁦Lazy_AMPR⁩ يحدّد الملفات التي تُحزم والملفات التي تبقى منفصلة. تُطبَّق آخر قاعدة مطابقة؛ وتبقى ملفات النظام منفصلة دائمًا.",
      warn: "قد تؤدي القواعد الخاطئة إلى لعبة لا تعمل. احتفظ بنسخة.",
      pickTitle: "استراتيجية الحزم (ملف TOML)",
      clear: "مسح",
    },
  },
  // PackHelp.tsx
  packHelp: {
    title: "كيف يعمل الحزم",
    close: "إغلاق",
    body: () => (
      <>
        <h3>ZSTD</h3>
        <ol>
          <li>يحزم أصول اللعبة في أجزاء ZSTD، في مجلد أو صورة.</li>
          <li><b>سريع</b> (ZSTD 5) سريع؛ أما <b>متوسط</b> (9) و<b>أقصى</b> (19) فأصغر وأبطأ.</li>
          <li>في لعبة محزومة بـ ZSTD، يؤدي اختيار <b>بلا</b> إلى فك حزمها: فتعود الملفات العادية.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>قد لا تعمل بعض الألعاب.</b> احتفظ بنسخة.</li>
          <li>يحزم في أجزاء LZ4، في مجلد أو صورة: <b>سريع</b> (LZ4)، <b>متوسط</b> (HC 9)، <b>أقصى</b> (HC 12).</li>
          <li>يعمل على ⁦ampr-emu-ng⁩، كما تعمل حزم ZSTD.</li>
          <li>في لعبة محزومة بـ LZ4، يؤدي اختيار <b>بلا</b> إلى فك حزمها.</li>
        </ol>

        <h3>⁦ampr-emu-ng⁩</h3>
        <ol>
          <li>بيئة تشغيل AMPR الوحيدة التي يضمّنها Forge. يقرأ حزم ZSTD و LZ4؛ ويثبّته الحزم في ⁦fakelib/⁩.</li>
          <li>يستبدل البناء به ⁦ampr_emu⁩ الخاص باللعبة، ما لم تُبقِ «الإعدادات» على ⁦ampr_emu⁩. لا يحمل FPKG بيئة تشغيل: يوفّرها مُحمِّل الجهاز.</li>
          <li>أوقف تحديث fakelib في {iso("ShadowMount+")}، وإلا فسيستبدل بيئة التشغيل ولن تتمكن اللعبة من قراءة حزمها.</li>
        </ol>

        <h3>استراتيجية الحزم</h3>
        <ol>
          <li><b>حزم كل شيء ما عدا ملفات النظام</b> (⁦ZSTD⁩ و⁦LZ4⁩): كل الملفات ما عدا ⁦eboot.bin⁩ والملفات التنفيذية والوحدات و⁦sce_sys⁩ و⁦sce_module⁩ و⁦fakelib⁩ وملفات ⁦.ini⁩ (يبقى ⁦dlc_emu.ini⁩ قابلًا للتعديل)؛ ويقدّم ⁦ampr-emu-ng⁩ الباقي، بما فيه الصوت والفيديو.</li>
          <li>عند الحزم: من <b>قواعد مخصّصة (TOML)</b> اختر ملف تعريف بصيغة ⁦ampr_pack.py⁩ و⁦Lazy_AMPR⁩، على هذا الجهاز أو على جهاز آخر.</li>
          <li>تُطبَّق آخر قاعدة مطابقة؛ وتبقى ملفات النظام منفصلة دائمًا. قد تؤدي القواعد الخاطئة إلى لعبة لا تعمل.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "الأجهزة",
    thisDevice: "هذا الجهاز",
    drives: "محركات الأقراص",
    location: "الموقع",
    parent: "المجلد الأعلى",
    allDrives: "كل محركات الأقراص",
    truncated: (n) =>
      plural(n, {
        zero: `لا يُعرض إلا أول ${num(n)} عناصر.`,
        one: "لا يُعرض إلا العنصر الأول.",
        two: "لا يُعرض إلا العنصران الأولان.",
        few: `لا يُعرض إلا أول ${num(n)} عناصر.`,
        many: `لا يُعرض إلا أول ${num(n)} عنصرًا.`,
        other: `لا يُعرض إلا أول ${num(n)} عنصر.`,
      }),
    loading: "جارٍ التحميل",
    noDrives: "لم يُعثر على محركات أقراص.",
    noFolders: "لا توجد مجلدات هنا.",
    noImages: "لا توجد هنا مجلدات أو صور PS5 أو أرشيفات.",
    noFiles: (ext) => `لا توجد هنا مجلدات أو ملفات ${dotted(ext, " أو ")}.`,
    chooseFiles: (n) => (n === 0 ? "اختيار ملفات" : `اختيار ${count(n, FILES_GEN)}`),
    nothing: "لا توجد هنا مجلدات أو ملفات.",
    cantList: "تعذّر فتح هذا المجلد:",
    cancel: "إلغاء",
    chooseFolder: "اختيار هذا المجلد",
    netOnlyHere: "الأرشيفات و⁦FPKG⁩ تُفتح من هذا الجهاز فقط.",
  },
  // bookmarks.tsx: the bookmarked folders (Transfer and the File explorer)
  bookmarks: {
    label: "الإشارات المرجعية",
    add: "إضافة هذا المجلد إلى الإشارات المرجعية",
    remove: "إزالة الإشارة المرجعية",
    removeNamed: (name: string) => `إزالة الإشارة المرجعية ${iso(name)}`,
    full: "9 إشارات مرجعية كحد أقصى",
  },
  // DeviceTabs.tsx
  deviceTabs: {
    add: "إضافة جهاز",
    offline: "غير متصل",
    offlineNamed: (name: string) => `${iso(name)} · غير متصل`,
  },
  // common.tsx
  common: {
    folder: "مجلد",
    pickButton: "اختيار",
    pickAny: "مجلد اللعبة أو صورتها أو أرشيفها",
    source: "المصدر",
    emptyTitle: "اختر مجلد لعبة أو صورة أو أرشيفًا",
    accepted: "المصادر المقبولة",
    unknownTitle: "عنوان غير معروف",
    unsigned: (n: number) =>
      plural(n, {
        zero: `${num(n)} ملفات تنفيذية غير موقّعة، لذا لن تبدأ اللعبة. تحويل اللعبة يوقّعها.`,
        one: "ملف تنفيذي واحد غير موقّع، لذا لن تبدأ اللعبة. تحويل اللعبة يوقّعه.",
        two: "ملفان تنفيذيان غير موقّعين، لذا لن تبدأ اللعبة. تحويل اللعبة يوقّعهما.",
        few: `${num(n)} ملفات تنفيذية غير موقّعة، لذا لن تبدأ اللعبة. تحويل اللعبة يوقّعها.`,
        many: `${num(n)} ملفًا تنفيذيًا غير موقّع، لذا لن تبدأ اللعبة. تحويل اللعبة يوقّعها.`,
        other: `${num(n)} ملف تنفيذي غير موقّع، لذا لن تبدأ اللعبة. تحويل اللعبة يوقّعها.`,
      }),
    patch: "تصحيح",
    additionalContent: "محتوى إضافي",
    reading: "جارٍ قراءة المصدر",
    unreadable: "مصدر غير قابل للقراءة",
    backport: "الباكبورت",
    emulators: "المحاكيات",
    declares: (fw) => `يُعلن ${fw}`,
    fwUnknown: "البرنامج الثابت غير معروف من ملفاته التنفيذية (لا يوجد SDK قابل للقراءة)",
    emulatorList: (names) => `المحاكيات: ${iso(names)}`,
    otherEmulator: "أخرى",
    sdk: (v) => `SDK ${v} (مُعلن)`,
    titleId: "معرّف اللعبة",
    version: "الإصدار",
    firmware: "البرنامج الثابت",
    fwDeclared: "معلن",
    fwBackport: "الباكبورت",
    fwExecutables: "الملفات التنفيذية",
    size: "الحجم",
    files: (n) => count(n, FILES),
    contentId: "معرّف المحتوى",
    noAmpr: "ملف ⁦eboot.bin⁩ لا يستورد libSceAmpr",
    traceRuntime: "بيئة تتبّع LZ4 القديمة",
    noRuntime: "لا توجد بيئة تشغيل",
    packed: (kind) => `محزومة (⁦${kind}⁩)`,
    zstdKept: "حزم أصول ZSTD: يُبقي عليها التحويل",
    damaged: "حزم تالفة",
    unpacked: "غير محزومة",
    packedDetail: (files, volumes, percent) =>
      `${count(files, FILES)} في ${count(volumes, { one: "جزء واحد", two: "جزأين", few: "أجزاء", many: "جزءًا", other: "جزء" })}` +
      (percent === null ? "" : `، ${num(percent)}% من الحجم`),
    index: {
      stale: "⁦ampr_emu.index⁩ قديم: يحدّثه أي تحويل",
      damaged: "⁦ampr_emu.index⁩ تالف: يعيد أي تحويل كتابته",
      missing: "لا يوجد ⁦ampr_emu.index⁩: يكتبه أي تحويل",
    },
    runtimeAction: (a: "install" | "replace" | "keep" | "keepIfPacks") =>
      a === "install"
        ? "سيثبّت أي تحويل ⁦ampr-emu-ng⁩ المضمّن"
        : a === "replace"
          ? "سيستبدل أي تحويل بيئة التشغيل بـ ⁦ampr-emu-ng⁩ المضمّن"
          : a === "keep"
            ? "يُبقي عليه أي تحويل (الإعدادات)"
            : "يُبقي عليه أي تحويل إن كان يقرأ حزم LZ4، وإلا فيستبدله بـ ⁦ampr-emu-ng⁩",
    backportLibs: (exclusive, n) =>
      `${exclusive ? "fakelib2/، يُركّب وحده" : "fakelib/"}، ${count(n, { one: "مكتبة باكبورت واحدة", two: "مكتبتا باكبورت", few: "مكتبات باكبورت", many: "مكتبة باكبورت", other: "مكتبة باكبورت" })}`,
    findings: "ملاحظات التحقق المسبق",
    dlcTitle: (n) => `محتوى DLC مضمّن (${num(n)})`,
    dlcLabel: "محتوى DLC مضمّن",
    dlcAll: (note) => `كل محتوى DLC: ${note}.`,
    dlcIn: (folder, size) => `في ${iso(folder + "/")}، ${size}`,
    dlcNoData: "يفتحه محاكي DLC (⁦dlc_emu.ini⁩): ترخيص فقط، دون بيانات خاصة به",
    dlcStatus: (status) => `يفتحه محاكي DLC (⁦dlc_emu.ini⁩)، الحالة: ${status || "غير محددة"}`,
    dlcMerged: "مدمج في مجلدات اللعبة",
    backportFiles: "ملفات الباكبورت",
    emulatorsIn: "المحاكيات في fakelib",
    qr: (url) => `رمز QR: ${iso(url)}`,
    needsPassword: "يحتاج هذا الأرشيف إلى كلمة مرور.",
    enterPassword: "إدخال كلمة المرور",
    inArchive: (ext, size) => `داخل أرشيف ${iso(ext)} · ${iso(size)}`,
    archive: "الأرشيف",
    parts: (n) => count(n, { one: "جزء واحد", two: "جزءان", few: "أجزاء", many: "جزءًا", other: "جزء" }),
    archiveRoot: (root) => `اللعبة في ${iso(root)}`,
    archiveAtRoot: "اللعبة في جذر الأرشيف",
    solid: "متصل (solid)",
    encrypted: "محمي بكلمة مرور",
  },
  // delete.tsx
  delete: {
    title: (folder) => (folder ? "حذف هذا المجلد؟" : "حذف هذه الصورة؟"),
    text: (folder) =>
      folder
        ? "سيُحذف المجلد وكل ما فيه نهائيًا: لا سلة مهملات ولا تراجع. إذا توقف الحذف في منتصفه، فقد يكون جزء منه قد حُذف بالفعل."
        : "سيُحذف ملف الصورة نهائيًا: لا سلة مهملات ولا تراجع.",
    no: "لا",
    yes: "نعم، احذف",
    archiveTitle: "حذف هذا الأرشيف؟",
    archiveText:
      "سيُحذف ملف الأرشيف نهائيًا: لا سلة مهملات ولا تراجع. تبقى الأجزاء الأخرى من الأرشيف متعدد الأجزاء كما هي.",
    fileTitle: "حذف هذا الملف؟",
    fileText: "سيُحذف الملف نهائيًا: لا سلة مهملات ولا تراجع.",
  },
  // replace.tsx
  replace: {
    title: (n) =>
      plural(n, {
        zero: `${num(n)} عناصر موجودة هناك بالفعل`,
        one: "عنصر واحد موجود هناك بالفعل",
        two: "عنصران موجودان هناك بالفعل",
        few: `${num(n)} عناصر موجودة هناك بالفعل`,
        many: `${num(n)} عنصرًا موجودًا هناك بالفعل`,
        other: `${num(n)} عنصر موجود هناك بالفعل`,
      }),
    inFolder: (folder) => <>في {folder}:</>,
    more: (n) =>
      `بالإضافة إلى ${count(n, { one: "عنصر آخر", two: "عنصرين آخرين", few: "عناصر أخرى", many: "عنصرًا آخر", other: "عنصر آخر" })}`,
    text: "يضع «استبدال» العناصر الجديدة مكانها: تُفقد الملفات القديمة نهائيًا، بلا سلة مهملات ولا تراجع. والمجلد الموضوع على مجلد بالاسم نفسه يُدمج معه: تُستبدل الملفات ذات الأسماء نفسها، ويبقى كل ما عداها هناك.",
    cancel: "إلغاء",
    replace: "استبدال",
  },
  // password.tsx
  password: {
    title: "يحتاج هذا الأرشيف إلى كلمة مرور",
    text: "أدخل كلمة المرور لفتحه.",
    wrong: "كلمة المرور غير صحيحة. تحقّق منها وحاول مرة أخرى.",
    label: "كلمة المرور",
    cancel: "إلغاء",
    unlock: "فتح القفل",
    checking: "جارٍ التحقق",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "تعذّر الوصول إلى مُحمِّل الحمولات على المنفذ 9021. شغّل أداة التحميل التلقائي (كسر الحماية) أولًا، ثم افتح Dump Forge مرة أخرى.",
      missing:
        "لم يُعثر على نسخة محفوظة من Dump Forge (في ⁦/data/dumpforge/⁩ أو على وحدة USB). حمّله مرة واحدة من كمبيوتر؛ وسيحفظ نسخة لتشغيله من هذه الأيقونة.",
      timeout:
        "بدأ تشغيل Dump Forge لكنه لم يستجب بعد 30 ثانية. ربما لا يزال قيد البدء (يكون الجهاز بطيئًا عند استيقاظه من وضع الراحة).",
      stopping: "لا يزال Dump Forge السابق قيد الإيقاف (يلغي مهامه وينظّف). انتظر قليلًا ثم أعد المحاولة.",
    },
    loadFailed: (e) => `تعذّر تحميل الصفحة: ${iso(e)}`,
    noAnswer: "لم يستجب Dump Forge",
    cantStart: "تعذّر تشغيل Dump Forge",
    retry: "إعادة المحاولة",
    starting: "جارٍ تشغيل Dump Forge…",
    client: {
      unreachable:
        "تعذّر الوصول إلى Dump Forge. تأكد من أن الحمولة لا تزال تعمل وأن هذا الجهاز على الشبكة نفسها.",
      noPlatform: "تعذّر تحديد ما إذا كان Dump Forge يعمل على PS5. حدّث الصفحة لإعادة المحاولة.",
      quitUnreachable: "تعذّر الوصول إلى Dump Forge لإيقافه.",
      stillAnswering: "لا يزال Dump Forge يستجيب بعد دقيقتين؛ تحقق من جهاز PS5.",
      gone: "لم يعد Dump Forge يُبلغ عنها (أُزيلت من سجل مهامه قبل ظهور نتيجتها)",
      uploadIncomplete: "لم يكتمل الرفع.",
    },
  },
  // target.tsx
  target: {
    kraken: {
      fast: { label: "سريع", note: { tone: "good", text: "مستحسن. أسرع بناء." } },
      balanced: {
        label: "متوازن",
        note: {
          text: "أصغر من «سريع» بنحو 2.6%، لكن الضغط يستغرق نحو 6 أضعاف الوقت، مع انشغال كل الأنوية طوال الوقت.",
        },
      },
      smallest: {
        label: "الأصغر",
        note: {
          text: "أصغر من «سريع» بنحو 2.7%، لكن الضغط يستغرق نحو 9 أضعاف الوقت، مع انشغال كل الأنوية طوال الوقت.",
        },
      },
      stored: {
        label: "دون ضغط",
        note: {
          text: "تكون الحزمة بحجم اللعبة تقريبًا. جرّبه إذا ثُبّتت حزمة مضغوطة لكنها لم تبدأ (أُبلغ عن ذلك على البرنامج الثابت 8.60 و10.40).",
        },
      },
    },
    imageInside: "الصورة الداخلية",
    imageInsideLabel: "الصورة داخل ffpfsc",
    compressionLevel: "مستوى الضغط",
    compression: "الضغط",
    fullVerify: "تحقق كامل",
    fullVerifyHint: "يعيد قراءة كل بايت للتحقق من الناتج. يستغرق وقتًا أطول.",
    deleteSource: "حذف المصدر",
    deleteSourceHint: "لا يمكن التراجع. لا يُحذف المصدر إلا بعد نجاح التحويل.",
    deleteSourceNas: "لا يمكن حذف لعبة على NAS من هنا.",
    separator: "لا يمكن أن يحتوي الاسم على فاصل مجلدات؛ استخدم «تغيير» لاختيار المجلد.",
    badName: "لا يمكن أن يكون الاسم فارغًا أو أن يبدأ بنقطة أو ينتهي بنقطة أو مسافة أو أن يحتوي على محارف تحكم.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `طويل جدًا: ${count(bytes, { one: "بايت واحد", two: "بايتان", few: "بايتات", many: "بايتًا", other: "بايت" })}، والحد الأقصى ${max} (UTF-8).`
        : `طويل جدًا: ${count(bytes, { one: "بايت واحد", two: "بايتان", few: "بايتات", many: "بايتًا", other: "بايت" })} قبل الامتداد، والحد الأقصى ${max} (UTF-8).`,
    pkgExt: (ext) => `ينتهي بـ ${iso(`.${ext}`)}، لكن FPKG يُحفظ بامتداد ${iso(".pkg")}: الامتداد هو ما يحدد برنامج التشغيل.`,
    otherExt: (ext, target) =>
      `ينتهي بـ ${iso(`.${ext}`)}، لكن الهدف ${target}: الامتداد هو ما يحدد برنامج التشغيل.`,
    extractInto: "الاستخراج إلى هذا المجلد",
    saveInto: "الحفظ في هذا المجلد",
    output: "الناتج",
    change: "تغيير",
    naming: "جارٍ اختيار الاسم…",
    chooseSourceFirst: "اختر مصدرًا أولًا",
    fileName: "اسم الملف",
    savedAs: (name, dir) => `سيُحفظ باسم ${iso(name)} في ${iso(dir)}`,
    inDir: (dir) => `في ${iso(dir)}`,
    generate: "إنشاء اسم من المحتوى",
  },
  // Formats.tsx (About's Output names) and target.tsx's name pattern: generated output names
  naming: {
    title: "أسماء المخرجات",
    parts: { title: "العنوان", title_id: "معرّف اللعبة", version: "الإصدار", pack: "نوع الحزم", region: "المنطقة", fw: "البرنامج الثابت" },
    template: "قالب الاسم",
    partsLabel: "إدراج جزء",
    insert: (part: string) => `إدراج ${part}`,
    inUse: (part) => `${part} موجود في القالب بالفعل`,
    example: "مثال",
    length: "الطول",
    bytes: (n, max) => `${n} من ${max} بايت.`,
    room: (n) => `يتبقى ${n} بايت للعنوان.`,
    noRoom: "لا تتبقى مساحة للعنوان: يُحذف العنوان.",
    versionOut: "يُحذف الإصدار لإفساح المجال.",
    cut: "يُقص الاسم عند 63 بايت.",
    idOnly: "نص القالب يملأ 63 بايت، لذا يُسمّى الإخراج بمعرّف اللعبة.",
    errors: {
      long: (max: number) => `لا يتجاوز القالب ${max} بايت.`,
      open: `توجد ${iso("{")} بلا ${iso("}")} تغلقها.`,
      close: `توجد ${iso("}")} بلا ${iso("{")} تفتحها.`,
      unknown: (token: string) => `${iso(`{${token}}`)} ليس جزءًا من الاسم: استخدم العنوان أو معرّف اللعبة أو الإصدار أو نوع الحزم أو المنطقة أو البرنامج الثابت.`,
      twice: (token) => `${iso(`{${token}}`)} مكرر في القالب: استخدم كل جزء مرة واحدة.`,
      char: (c: string) => `لا يمكن أن يحتوي الاسم على ${iso(c)}.`,
      control: "لا يمكن أن يحتوي الاسم على حرف تحكم.",
      needsName: "يحتاج الاسم إلى العنوان أو معرّف اللعبة.",
    },
    notSaved: "لم يُحفظ: تبقى الأسماء على آخر قالب صالح.",
    reset: "إعادة الضبط",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `يتوفر ${iso(`v${v}`)}`,
    update: "تحديث",
    later: "لاحقًا",
    updating: "جارٍ التحديث…",
    updatingNote: "يجري تنزيل الإصدار الجديد. سيُعاد تحميل هذه الصفحة بالإصدار الجديد تلقائيًا.",
    downloading: "يُفتح التنزيل في متصفحك: ضع الملف الذي نزّلته مكان هذه النسخة (على Mac، اسحب التطبيق من صورة القرص إلى مجلد التطبيقات).",
    downloadingCli: "يُفتح التنزيل في المتصفح: استبدل أداة سطر الأوامر ⁦ps5-dump-forge⁩ بالملف الذي نزّلته، أو شغّل ⁦ps5-dump-forge update --install⁩ على الحاسوب الذي يعمل عليه.",
    failed: (v) => `تعذّر التحديث إلى ${iso(`v${v}`)}`,
    timeout: (v) => `لم يبدأ الإصدار ${iso(`v${v}`)} خلال دقيقة. إذا اكتمل تنزيله، فأوقف Dump Forge وافتحه من جديد لإكمال التحديث.`,
    retry: "إعادة المحاولة",
    close: "إغلاق",
  },
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "هذا الجهاز",
    howItWorks: "كيفية الاتصال",
    ipAddress: "عنوان IP",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? `آخر ظهور: منذ ${count(n, MINUTES)}`
        : unit === "h"
          ? `آخر ظهور: منذ ${count(n, HOURS)}`
          : unit === "d"
            ? `آخر ظهور: منذ ${count(n, DAYS)}`
            : "آخر ظهور: الآن",
    listenError: (e) => `لا تستطيع الأجهزة الأخرى الوصول إلى هذا الجهاز: ${iso(e)}`,
    unavailable: (e) => `الاتصال غير متاح: ${iso(e)}`,
    loading: "جارٍ التحميل",
    discover: "الاكتشاف التلقائي",
    stop: "إيقاف",
    fileTransfer: "نقل الملفات",
    devices: "الأجهزة",
    show: "عرض",
    all: "الكل",
    onlineOf: (n, total) => `${num(n)} من ${num(total)} متصل`,
    sendFilesTo: (name) => `إرسال ملفات إلى ${iso(name)}`,
    userIs: (user) => `المستخدم ${iso(user)}`,
    nearby: "بالقرب منك",
    nearbyNone: "لم يُعثر على أجهزة جديدة",
    addDevice: "إضافة جهاز",
    addressExample: "192.168.1.20 أو nas.local",
    addByProtocol: "أو أضف حسب البروتوكول:",
    savedEmpty: "لا توجد أجهزة محفوظة بعد",
    savedEmptyHint:
      "يعثر «الاكتشاف التلقائي» على Forge والتخزين الشبكي في هذه الشبكة. أقرِن جهازًا واسمح بالإقران عليه، أو احفظ NAS.",
    online: "متصل",
    offline: "غير متصل",
    computer: "كمبيوتر",
    address: "العنوان",
    keyId: "معرّف المفتاح",
    lastSeen: "آخر ظهور",
    status: "الحالة",
    never: "أبدًا",
    unknown: "غير معروف",
    forget: "نسيان",
    forgetNote: "يزيل «نسيان» الإقران من هذا الجهاز فقط. لاستخدامه مرة أخرى، أقرِنه من جديد.",
    close: "إغلاق",
    cancel: "إلغاء",
    pairTitle: "إقران جهاز",
    pair: "إقران",
    connectTo: "اتصال",
    probing: (address) => `جارٍ التحقق من ${iso(address)}`,
    pairing: "جارٍ الإقران",
    waitingFor: (address) => `في انتظار أن يسمح ${iso(address)} بالإقران`,
    checkNumber: "رقم التحقق",
    checkHint: "يجب أن يطابق الرقم الذي يعرضه الجهاز الآخر.",
    denied: "رفض الجهاز الآخر الإقران.",
    timedOut: "لم يسمح أحد بالإقران على الجهاز الآخر في الوقت المحدد.",
    pairBusy: "لدى الجهاز الآخر طلبات إقران أخرى في الانتظار. أعد المحاولة بعد دقيقة.",
    alreadyPaired: "ذلك الجهاز حفظ هذا الجهاز من قبل. انسَه هناك أولًا، ثم أقرِنه من جديد.",
    tooManyAttempts: "طلبات مرفوضة كثيرة جدًا. أعد المحاولة بعد 10 دقائق.",
    pairPromptTitle: (name) => `هل تريد الإقران مع ${iso(name)}؟`,
    pairPromptBody: (name, platform, ip) =>
      `يريد ${iso(name)} (${iso(platform)}، ${iso(ip)}) الإقران مع هذا الجهاز. يستطيع الجهاز المقترن قراءة ملفات هذا الجهاز وإضافة ملفات جديدة.`,
    pairPromptCheck: (name) => `لا تسمح بذلك إلا إذا عرض ${iso(name)} رقم التحقق نفسه.`,
    allow: "سماح",
    deny: "رفض",
    notPaired: "لم يعد الجهاز الآخر يعرف هذا الجهاز. أقرِنه من جديد.",
    searching: "جارٍ البحث في هذه الشبكة",
    forge: "Forge",
    save: "حفظ",
    kindNas: "NAS",
    nasAddTitle: (protocol) => `إضافة NAS عبر ${protocol}`,
    nasEditTitle: (protocol) => `تعديل NAS عبر ${protocol}`,
    nasName: "الاسم",
    nasAddressHint: "عنوان IP أو اسم المضيف؛ أضف :المنفذ إن لم يكن 445",
    nasUser: "اسم المستخدم",
    nasUserHint: "اتركه فارغًا للدخول كضيف",
    ftpAddressHint: "عنوان IP أو اسم المضيف؛ أضف :المنفذ إن لم يكن 21",
    nfsAddressHint: "عنوان IP أو اسم المضيف؛ أضف :المنفذ إن لم يكن 2049",
    ftpUserHint: "اتركه فارغًا للدخول دون اسم",
    ftpNote: "لا يُشفَّر FTP، ولا بيانات الدخول كذلك.",
    nfsNote: "لا يحتاج NFS إلى تسجيل دخول: يتصل Forge بصلاحيات الجذر (NFSv4.1، وإلا NFSv3)، لذا يجب أن يسمح التصدير على الخادم لهذا الجهاز. لا يُشفَّر NFS.",
    nasPassword: "كلمة المرور",
    nasPasswordKeep: "اتركها فارغة للإبقاء على كلمة المرور المحفوظة",
    guest: "ضيف",
    passwordSaved: "محفوظة",
    passwordNone: "لا توجد",
    edit: "تعديل",
    nasForgetNote: "يزيل «نسيان» هذا الـ NAS وكلمة مروره المحفوظة من هذا الجهاز.",
    thisBrowser: "هذا المتصفح",
    uploadPick: "اختيار ملفات للرفع",
    uploading: (name, i, n) => `جارٍ رفع ${iso(name)} (${num(i)} من ${num(n)})`,
    uploaded: "اكتمل الرفع",
    uploadSkipped: (names) => `لم يُرفع، إذ يوجد هناك مجلد أو رابط بالاسم نفسه: ${iso(names)}.`,
    uploadedBefore: (done, n) => `الملفات المرفوعة: ${num(done)} من ${num(n)}. تبقى على هذا الجهاز.`,
    cantUpload: "تعذّر الرفع:",
    send: "إرسال",
    downloadHere: "التنزيل من هذا الجهاز",
    wholeRoot: "لا يمكن إرسال جهاز أو مشاركة أو محرك أقراص بالكامل: اختر مجلدًا داخله.",
    helpPhone: "امسح رمز QR لفتح هذه الصفحة على هاتف. هناك، يرفع «النقل» ملفات الهاتف إلى هذا الجهاز وينزّل ملفات هذا الجهاز إلى الهاتف.",
    help: [
      "يربط «الاتصال» نسخ Dump Forge على جهاز PS5 وأجهزة الكمبيوتر لديك في الشبكة نفسها.",
      "للإقران، شغّل «الاكتشاف التلقائي» واختر «إقران» في «بالقرب منك»، أو اكتب عنوانًا في «إضافة جهاز»، ثم اسمح بذلك على الجهاز الآخر. لا تسمح إلا إذا عرضت الشاشتان رقم التحقق نفسه.",
      "يُضاف NAS (SMB وFTP وNFS) بالطريقة نفسها. الأجهزة المقترنة وأجهزة NAS مصادر للألعاب ووجهات للإخراج معًا، ولا تُعدَّل ألعابها هناك.",
      "ينسخ «النقل»، في القائمة الجانبية، الملفات والمجلدات بين أي منها. ولا يُستبدل أي اسم موجود بالفعل في الوجهة إلا بعد تأكيدك.",
    ],
    helpSecurity: "لا شيء مما يُرسَل بين الأجهزة مشفّر: يمكن لأي شخص على شبكتك قراءته. وتُحفَظ كلمة مرور NAS على هذا الجهاز دون تشفير. استخدم «الاتصال» على شبكتك المنزلية فقط.",
    helpSpeed: "يرسل «اختبار السرعة»، في صف جهاز مقترن أو في تفاصيله، ⁦128 MiB⁩ إليه ويستلم ⁦128 MiB⁩ منه، دون كتابة شيء على القرص، ويعرض سرعة الشبكة في كل اتجاه.",
    speedTest: "اختبار السرعة",
    speedTestWith: (name) => `اختبار السرعة مع ${iso(name)}`,
    speedTesting: "جارٍ الاختبار",
    speedResult: (up, down) => `الرفع ${iso(`${up} MB/s`)} · التنزيل ${iso(`${down} MB/s`)}`,
    speedFailed: "فشل اختبار السرعة:",
    speedNote: "يرسل ⁦128 MiB⁩ ويستلم ⁦128 MiB⁩ منه؛ لا يُكتب شيء على القرص.",
    installTitle: "Dump Forge على PS5",
    ps5Address: "عنوان PS5",
    install: "التثبيت على PS5",
    installHint: "يتطلب مُحمِّل ELF على الجهاز يستمع على الشبكة (المنفذ 9021).",
    findingRelease: "جارٍ البحث عن الإصدار على GitHub",
    installing: (v, host) => `جارٍ تثبيت Dump Forge ${v} على ${iso(host)}`,
    installed: (v, host) => `يبدأ Dump Forge ${v} على ${iso(host)}. اقرنه من «إضافة جهاز» بعد تشغيله.`,
    noRelease: "تعذّر العثور على إصدار Dump Forge لـ PS5 على GitHub. تحقّق من اتصال هذا الكمبيوتر بالإنترنت، ثم أعد المحاولة.",
    otherPayload: "أي حمولة ELF أخرى:",
    sendPayload: "إرسال حمولة",
    pickPayload: "اختر حمولة ELF",
    sendingPayload: (name, host) => `جارٍ إرسال ${iso(name)} إلى ${iso(host)}`,
    sentPayload: (name, size, host) => `أُرسلت ${iso(name)} ‏(${size}) إلى ${iso(host)}.`,
    helpInstallTitle: "ثبّت Dump Forge على PS5 بنقرة واحدة",
    helpInstallText: "استخدم بطاقة تثبيت Dump Forge لإرسال الحمولة إلى الجهاز. يُنزّل الإصدار المطابق من GitHub (أو الأحدث) ويشغّله.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "مثبّت الحزم",
    packages: "حزم التثبيت",
    refresh: "تحديث",
    scanning: "جارٍ البحث عن الحزم",
    none: "لم يُعثر على حزم على هذا الجهاز.",
    noneHint: `انسخ ملف ${iso(".pkg")} إلى ذاكرة الجهاز أو إلى ذاكرة USB، ثم اضغط «تحديث».`,
    truncated: "قد تكون القائمة غير مكتملة",
    limits: "توقّف البحث عند أحد حدوده: قد تنقص بعض الحزم.",
    cantScan: "تعذّر البحث:",
    cantInstall: "تعذّر التثبيت:",
    content: { game: "لعبة", patch: "تصحيح", additional_content: "محتوى إضافي", unknown: "غير معروف" },
    install: "تثبيت",
    installName: (name) => `تثبيت ${iso(name)}`,
    started: (name: string) => `جارٍ تثبيت ${iso(name)}. تظهر المهمة في «قائمة الانتظار».`,
    tag: "تثبيت",
    title: "تثبيت هذه الحزمة؟",
    titleOn: (device) => `تثبيت هذه الحزمة على ${iso(device)}؟`,
    file: "الملف",
    game: "اللعبة",
    titleId: "معرّف اللعبة",
    version: "الإصدار",
    size: "الحجم",
    text: "يجلب PS5 الحزمة من Dump Forge ويثبّتها. أبقِ Dump Forge قيد التشغيل حتى تنتهي المهمة.",
    reinstall: "إذا كانت هذه اللعبة مثبّتة بالفعل، يحذف PS5 النسخة المثبّتة قبل أن يثبّت هذه.",
    cancel: "إلغاء",
    confirm: "تثبيت",
    installPkg: "تثبيت pkg",
    installPkgOn: (device) => `تثبيت حزمة على ${iso(device)}`,
    pickTitle: "اختر حزمة لتثبيتها",
    sent: (device: string) => `يثبّت ${iso(device)} الحزمة. تظهر المهمة في «قائمة الانتظار».`,
  },
  // Build's and Edit's Restore metadata online switch (build/options.tsx, Edit.tsx)
  metadata: {
    tag: "البيانات الوصفية",
    cant: "تعذّرت استعادة البيانات الوصفية:",
    label: "استعادة البيانات الوصفية عبر الإنترنت",
    buildHint: `قبل البناء، يُنزّل ${iso("param.json")} والصور والصوت وجداول ${iso("PlayGo")} لهذا الإصدار من خادم تحديثات ${iso("Sony")} ويضعها في الناتج حيث تختلف. لا تتغيّر اللعبة نفسها. يحتاج إلى الإنترنت؛ ومن دونه يستمر البناء كما هو.`,
    buildSummary: `يضع البيانات الوصفية من ${iso("Sony")} في الناتج (لا تتغيّر اللعبة)`,
    editHint: `يطبّق على هذه اللعبة، مع تغييراتك، ${iso("param.json")} والصور والصوت وجداول ${iso("PlayGo")} لهذا الإصدار من خادم تحديثات ${iso("Sony")} حيث تختلف: يتغيّر المجلد مباشرة، و${iso("exfat")} أو ${iso("ffpkg")} في مكانه، ويُعاد بناء ${iso("ffpfs")} أو ${iso("ffpfsc")} أو ${iso("FPKG")}. يحتاج إلى الإنترنت؛ إذا فشل التنزيل فلا يتغيّر شيء.`,
    editOnly: `البيانات الوصفية من ${iso("Sony")} لهذا الإصدار، مطبّقة على هذه اللعبة`,
    editAlso: `تُطبَّق أيضًا البيانات الوصفية من ${iso("Sony")} لهذا الإصدار، بعد تنزيلها أولًا.`,
    editConfirm: `تطبيق البيانات الوصفية من ${iso("Sony")}؟`,
    editConfirmText: "تُنزَّل البيانات الوصفية أولًا؛ وإذا فشل ذلك فلا يتغيّر شيء.",
  },
  // App.tsx: the side rail
  rail: {
    build: "البناء",
    settings: "الإعدادات",
    queue: "قائمة الانتظار",
    transfer: "النقل",
    collapse: "طيّ القائمة",
    expand: "توسيع القائمة",
  },
  // Queue.tsx
  queue: {
    empty: "لا توجد مهام بعد. تظهر هنا مهام البناء والتحرير والنقل والتثبيت.",
    counts: (running: number, queued: number, failed: number, done: number): string =>
      [
        running && `قيد التشغيل: ${num(running)}`,
        queued && `في الانتظار: ${num(queued)}`,
        failed && `فشلت: ${num(failed)}`,
        done && `اكتملت: ${num(done)}`,
      ]
        .filter(Boolean)
        .join("، ") || "لا شيء لعرضه",
    retryFailed: (n: number): string => (n > 0 ? `إعادة محاولة الفاشلة (${num(n)})` : "إعادة محاولة الفاشلة"),
    retrying: (i: number, n: number) => `تجري إعادة المحاولة: ${num(i)} من ${num(n)}`,
    clearFinished: "مسح المنتهية",
    filters: { all: "الكل", running: "قيد التشغيل", failed: "فشلت", done: "اكتملت" },
    filterLabel: "عرض",
    noneShown: "لا توجد مهام تطابق هذا المرشّح.",
  },
  // Settings.tsx
  settings: {
    appearance: "المظهر",
    theme: "السمة",
    themeHint: "الداكنة تناسب التلفاز، والفاتحة تناسب غرفة مضيئة.",
    dark: "داكنة",
    light: "فاتحة",
    scheme: "نظام الألوان",
    schemeHint: "لون التمييز في التطبيق كله.",
    schemes: { abyss: "الهاوية", ember: "الجمر", moss: "الطحلب", cobalt: "الكوبالت", dusk: "الغسق", rose: "الورد", sand: "الرمل", volt: "فولت", mono: "أحادي اللون" },
    advanced: "خيارات متقدمة",
    advancedOpen: "إظهار الخيارات المتقدمة دائمًا",
    advancedHint:
      "يعرض كل الحزم والصيغ دون «المزيد»، ويُبقي كل الأقسام المطوية مفتوحة. أوقفه ليبقى «البناء» قصيرًا.",
    bundled: (version: string) => `${version} (مضمّنة)`,
    replaceAmprEmu: "استبدال ampr_emu بـ ampr-emu-ng",
    languageHint: "لغات النظام إلى أن تختار لغة.",
    notificationsGroup: "PS5",
  },
  // JobList.tsx: a finished job's result
  result: {
    file: "الناتج",
    type: "النوع",
    size: "الحجم",
    files: "الملفات",
    time: "الوقت",
    verified: "تم التحقق",
    verifiedFast: "تم التحقق (سريع)",
    verifiedFull: "تم التحقق (كامل)",
    notVerified: "لم يُبلَّغ عن أي فحص",
    checks: "الفحوص:",
    copy: "نسخ التفاصيل",
    copied: "تم النسخ",
    copyFailed: "لا يمكن النسخ هنا",
    ofSource: (pct: number) => `${num(pct, 1)}% من ملفات اللعبة`,
    filesN: (n: number) => count(n, FILES),
    to: "كُتب في",
    took: (d: string) => `استغرق ${d}`,
    kinds: {
      pkg: "FPKG: قابلة للتثبيت على PS5 يدعم FPKG",
      exfat: "صورة exfat: يركّبها ShadowMount+، قابلة للكتابة",
      ffpkg: "صورة ffpkg ‏(UFS2): يركّبها ShadowMount+، قابلة للكتابة",
      ffpfs: "صورة ffpfs ‏(PFS): يركّبها ShadowMount+",
      ffpfsc: "صورة ffpfsc ‏(PFS مضغوط): يركّبها ShadowMount+ للقراءة فقط",
      folder: "مجلد اللعبة",
    } as Record<string, string | undefined>,
    zstdFolder: "مجلد بحزم ZSTD",
    lz4Folder: "مجلد بحزم LZ4",
    withPacks: (image: string, kind: string) => `${image}، والملفات في حزم ${kind}`,
    edited: (what: string) => `${what}، عُدّل في مكانه`,
    rebuilt: (what: string) => `${what}، أُعيد بناؤه`,
    extracted: "الملفات المستخرجة",
    transferred: "الملفات المنسوخة",
    installed: "سُلّمت الحزمة إلى مثبّت PS5",
  },
  // JobList.tsx: the log panel
  log: {
    lines: (n: number, of?: number): string =>
      of === undefined ? count(n, LINES) : `${num(n)} من ${count(of, LINES)}`,
    filter: "تصفية",
    follow: "تمرير تلقائي",
    copy: "نسخ",
    copied: "تم النسخ",
    copyFailed: "فشل النسخ",
    save: "حفظ",
    clear: "مسح",
    cleared: "تم المسح",
  },
  // common.tsx: the source card's fact chips
  sourceChips: {
    sdk: (v: string) => `SDK ${v}`,
    firmware: (v: string) => `FW ${v}+`,
  },
  // prefs.tsx: Advanced's badge
  advanced: {
    changed: "مُعدّلة",
    defaults: "الافتراضية",
  },
  // build/*: the Build screen
  build: {
    summary: "الملخص",
    addToQueue: "إضافة إلى قائمة الانتظار",
    queued: "أُضيف إلى قائمة الانتظار",
    openQueue: "فتح قائمة الانتظار",
    space: (free: string, dir: string, needed: string) => (
      <>
        المساحة الحرة في <span className="path">{iso(dir)}</span>: {free}، والمطلوب نحو {needed}
      </>
    ),
    needed: (needed: string) => `المطلوب نحو ${needed}`,
    spaceRaw: "تُفحص المساحة كأن لا شيء مضغوط.",
    estimate: "تقدير",
    advanced: "خيارات متقدمة",
    options: "الخيارات",
    fastVerify: "تحقق سريع: تُفحص كل البُنى، وتُعاد قراءة عيّنة من البيانات.",
    fullVerify: "تحقق كامل: تُعاد قراءة كل بايت.",
    deleted: "يُحذف المصدر بعد نجاح البناء. لا يمكن التراجع.",
    zstdBackport:
      "أصول هذه اللعبة محزومة بـ ZSTD وفهرس ZSTD يتضمّن الباكبورت: ابنِها مرة واحدة بخيار «الحزم: بلا»، ثم أزل الباكبورت.",
    lz4Backport:
      "أصول هذه اللعبة محزومة بـ LZ4 وبيان LZ4 يتضمّن الباكبورت: ابنِها مرة واحدة بخيار «الحزم: بلا»، ثم أزل الباكبورت.",
    backportLeftOut: (n: number) => `يُستبعد من ⁦fakelib/⁩ ${count(n, BACKPORT_LIBS)}.`,
    packsTitle: "الهدف",
    packs: "الحزم",
    more: "المزيد",
    moreTip: "صيغ أخرى",
    cards: {
      folder: "ملفات عادية، تُنسخ إلى أي مكان، ويشغّلها ShadowMount+",
      pkg: "تُثبَّت على PS5 مثل حزمة لعبة",
      exfat: "يركّبها ShadowMount+ وتُفتح على الحاسوب",
      ffpkg: "صورة UFS2 لـ ShadowMount+، ويمكن تركيبها للكتابة",
      ffpfs: "صورة PFS بلا ضغط، بأسماء ملفات ASCII فقط",
      ffpfsc: "أصغر صورة: مضغوطة، وتُركَّب دائمًا للقراءة فقط",
      lz4: "أصول محزومة بـ LZ4، في مجلد عادي",
    },
    zstdFolderCard: "أصول محزومة بـ ZSTD، في مجلد عادي",
    packCards: {
      nonePlain: "ملفات اللعبة كما هي",
      unpack: "يعيد كتابة الحزم ملفاتٍ عادية",
      keepZstd: "تُبقى حزم ZSTD بايتًا ببايت",
      keepLz4: "تُبقى حزم LZ4 بايتًا ببايت",
      zstd: "لعبة أصغر وتحميل أسرع، مع ampr-emu-ng",
      repackZstd: "يفك حزم LZ4 ويحزمها من جديد بـ ZSTD",
      lz4: "يحزم الأصول بـ LZ4، مع ampr-emu-ng",
      repackLz4: "يفك حزم ZSTD ويحزمها من جديد بـ LZ4",
    },
    naming: "الناتج: يجري إنشاء الاسم",
    output: (name: string, dir: string) => (
      <>
        الناتج: <b className="path">{iso(name)}</b> في <span className="path">{iso(dir)}</span>
      </>
    ),
    choices: { none: "بلا", zstd: "ZSTD", lz4: "LZ4" },
    levelLabel: (level: string, engine: string, n: number) => `${level}، ${engine} ${n}`,
    levelTip: (engine: string, n: number, note: string) => `${engine} ${n}. ${note}`,
    sumKeepZstd: "تُبقى حزم ZSTD بايتًا ببايت.",
    sumKeepLz4: "تُبقى حزم LZ4 بايتًا ببايت.",
    sumKeepPlain: "لا تُنشأ حزم: تُحوَّل ملفات اللعبة كما هي.",
    sumUnpack: (engine: string) => `تُعاد كتابة حزم ${engine} ملفاتٍ عادية.`,
    sumPack: (engine: string, level: string) => `تُحزم ملفات اللعبة في أجزاء ${engine} (${level}).`,
    sumRepack: (from: string, to: string, level: string) => `تُفك حزم ${from} وتُحزم من جديد بـ ${to} (${level}).`,
    sumRules: (name: string) => `تختار القواعد المخصصة ${iso(name)} الملفات التي تُحزم.`,
    sumInside: (format: string, level: number) => `في الداخل: ${format}، مستوى zlib ${level}.`,
    ngInstalled: "يُثبَّت ampr-emu-ng في ⁦fakelib/libSceAmpr.sprx⁩.",
    ngReplaced: (old: string) => `يُستبدل ${iso(old)} الخاص باللعبة بـ ampr-emu-ng.`,
    ngReplacedTrace: "تُستبدل بيئة تشغيل التتبّع القديمة الخاصة باللعبة بـ ampr-emu-ng.",
    ngKept: (old: string) => `يُبقى ${iso(old)} الخاص باللعبة.`,
    ngUpdated: (v: string) => `يُستبدل ${iso(`ampr-emu-ng${v}`)} الخاص باللعبة بالنسخة المضمّنة.`,
    ngSame: (v: string) => `يُبقى ${iso(`ampr-emu-ng${v}`)} الخاص باللعبة.`,
    pkgNoRuntime: "تُستبعد بيئة تشغيل AMPR من FPKG: يوفّرها مُحمِّل الجهاز.",
    recommended: "موصى به",
    resultTitle: "تقريبًا، بعد البناء",
    resultNow: "اللعبة الآن",
    resultCustomRules: "قواعد مخصصة: بلا تقدير",
    backportTitle: "الباكبورت",
    backportNone: "لا يوجد باكبورت في هذه اللعبة.",
    backportPlain: "للتحويل العادي فقط: دون إنشاء حزم أو فكّها.",
  },
  // TransferPane.tsx: one device's pane of the Transfer screen
  transfer: {
    pane: (device) => `الملفات على ${iso(device)}`,
    roots: "محركات الأقراص",
    location: "الموقع",
    up: "أعلى",
    upTip: "الصعود مجلدًا واحدًا",
    refresh: "تحديث",
    filter: "تصفية حسب الاسم",
    filterLabel: "تصفية هذا المجلد حسب الاسم",
    inFolder: (dirs, files) => `${count(dirs, FOLDERS)}، ${count(files, FILES)}`,
    matches: (n) => count(n, RESULTS),
    firstOf: (max, n) => `أول ${num(max)} من ${num(n)}`,
    truncated: "يحتوي هذا المجلد على أكثر من 10,000 عنصر: لا يُعرض منها إلا أول 10,000.",
    name: "الاسم",
    size: "الحجم",
    select: (name) => `تحديد ${iso(name)}`,
    open: (name) => `فتح ${iso(name)}`,
    selected: (n) => `المحدد: ${count(n, ITEMS)}`,
    clear: "مسح",
    clearTip: "مسح التحديد",
    dragItems: (n) => count(n, ITEMS),
    dropHere: (path) => `أفلِت للإرسال إلى ${iso(path)}`,
    notHere: "افتح مجلدًا لإرسال الملفات إليه.",
    emptyFolder: "هذا المجلد فارغ.",
    noMatch: (f) => `لا شيء هنا يطابق «${iso(f)}».`,
    noRoots: "لا توجد محركات أقراص متاحة للتصفّح.",
    cantList: "تعذّر فتح هذا المجلد:",
    loading: "جارٍ التحميل",
    link: "رابط",
    linkTip: "رابط: يمكن فتحه، لكن لا يمكن تحديده",
    mounted: "مُركَّب",
    mountedTip: `مُركَّب بواسطة ${iso("ShadowMount+")}: أوقف اللعبة وأوقف تشغيل ${iso("ShadowMount+")} لتحديده`,
    withTitle: "النقل مع",
    loadingDevices: "جارٍ تحميل الأجهزة",
    noneTitle: "لا يوجد جهاز للنقل معه",
    noneHint: "أقرِن Dump Forge آخر أو أضف NAS من «الاتصال»، أو شغّل جهازًا محفوظًا.",
    openConnect: "فتح «الاتصال»",
    deviceOffline: (device) => `${iso(device)} غير متصل. ستظهر ملفاته مجددًا عند عودته.`,
    copyTo: (n, device) => `نسخ ${count(n, ITEMS)} إلى ${iso(device)}`,
    copyNoFolder: (device) => `افتح على ${iso(device)} المجلد الذي تريد النسخ إليه.`,
    installOn: (device) => `التثبيت على ${iso(device)}`,
    dropFiles: "أفلِت الملفات هنا لرفعها",
    uploadInto: (folder) => `تذهب إلى ${iso(folder)}، المجلد المعروض على هذا الجهاز.`,
    uploadNoFolder: "افتح مجلدًا على هذا الجهاز للرفع إليه.",
    foldersLeftOut: (names) => `لا تُرفع المجلدات، بل الملفات فقط: ${iso(names)}.`,
    downloadFiles: (n) => `تنزيل ${count(n, FILES)}`,
    downloadNone: "حدّد ملفات على هذا الجهاز لتنزيلها (لا تُنزَّل المجلدات).",
  },
  duration: { s: (s) => `${s} ث`, ms: (m, s) => `${m} د ${s} ث`, hm: (h, m) => `${h} س ${m} د` },
};

export default ar;
