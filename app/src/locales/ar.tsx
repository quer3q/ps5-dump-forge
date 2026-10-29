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

const ar: Messages = {
  // App.tsx
  app: {
    screens: "الأقسام",
    convert: "تحويل",
    active: (n) => `، النشطة: ${num(n)}`,
    edit: "تحرير",
    pack: "الحزم",
    storage: "التخزين",
    connect: "الاتصال",
    disconnected: "غير متصل",
    connected: (n: number) => `متصل (${count(n, DEVICES)})`,
    about: "حول",
    prevTab: "التبويب السابق (⁦L2⁩)",
    nextTab: "التبويب التالي (⁦R2⁩)",
    offline: "غير متصل، جارٍ إعادة المحاولة…",
    stop: "إيقاف Dump Forge",
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
  // Convert.tsx
  convert: {
    target: "الهدف",
    targetFormat: "الصيغة الهدف",
    alreadyFolder: "المصدر مجلد بالفعل",
    lz4Backport:
      "أصول هذه اللعبة محزومة بـ LZ4، وبيان LZ4 الخاص بها يتضمن الباكبورت: فُكّ حزمها أولًا في تبويب «الحزم»، ثم أزل الباكبورت هنا.",
    zstdBackport:
      "أصول هذه اللعبة محزومة بـ zstd، وفهرس zstd الخاص بها يتضمن الباكبورت: فُكّ حزمها أولًا في تبويب «الحزم»، ثم أزل الباكبورت هنا.",
    unreadable: "لا يمكن البناء: تعذّرت قراءة هذا المصدر (راجع «المصدر»).",
    patchBlocked: "لا يمكن البناء: هذه حزمة تصحيح لا تحتوي إلا على ما غيّره التحديث. افتحها في «تحرير» لاستخراج ملفاتها.",
    openInEdit: "فتح في «تحرير»",
    noContentId: "لا يمكن البناء بصيغة fpkg: لا يحتوي ⁦param.json⁩ لهذه اللعبة على Content ID، وهو ما تحتاجه الحزمة.",
    blocked: (first, more) =>
      `لا يمكن البناء: ${iso(first)}${more > 0 ? ` (بالإضافة إلى ${count(more, { one: "ملاحظة أخرى", two: "ملاحظتين أخريين", few: "ملاحظات أخرى", many: "ملاحظةً أخرى", other: "ملاحظة أخرى" })}، راجع «المصدر»)` : ""}.`,
    amprZstdRuntime: "يثبّت هذا التحويل ⁦ampr-emu-ng⁩ في ⁦fakelib/libSceAmpr.sprx⁩، وهذه الحزم تحتاج إليه.",
    amprNgReplaced: (version) => `يستبدل ⁦ampr-emu-ng⁩ الخاص بهذه اللعبة بنسخة ⁦ampr_emu⁩ ${iso(version)}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `تستخدم هذه اللعبة AMPR لكن لا يوجد فيها ⁦ampr_emu⁩: سيثبّت التحويل نسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`;
      if (a === "trace") return `سيستبدل التحويل ⁦ampr_emu⁩ الخاص بهذه اللعبة بنسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`;
      if (a === "replace")
        return own
          ? `سيستبدل التحويل ⁦ampr_emu⁩ ${iso(own)} الخاص بهذه اللعبة بنسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`
          : `سيستبدل التحويل ⁦ampr_emu⁩ الخاص بهذه اللعبة، الذي لا يذكر رقم إصداره، بنسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`;
      return own
        ? `إصدار ⁦ampr_emu⁩ في هذه اللعبة ${iso(own)} مماثل للنسخة المضمّنة ${iso(version)} أو أحدث منها: سيُبقي عليه التحويل.`
        : `تحتوي هذه اللعبة بالفعل على نسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}: سيُبقي عليها التحويل.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "لا يطابق ⁦ampr_emu.index⁩ ملفات هذه اللعبة" : state === "damaged" ? "تتعذّر قراءة ⁦ampr_emu.index⁩" : "لا يوجد ⁦ampr_emu.index⁩ في هذه اللعبة"}: سيكتب التحويل فهرسًا جديدًا.`,
    amprPkgIndex: "ستحصل الحزمة على ⁦ampr_emu.index⁩ جديد مبني من ملفاتها.",
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
      zstd: { tagline: "للألعاب التي تستخدم AMPR، مع ampr-emu-ng", cells: ["تحميل أسرع بنسبة تصل إلى 80%", "حتى نصف حجم اللعبة"] },
      lz4: {
        tagline: "تجريبي، للألعاب التي تستخدم AMPR",
        cells: [
          "تحميل أسرع بنسبة 30–40%، حسب أرقام المطوّرين الأصليين",
          "نحو نصف حجم اللعبة، حسب أرقام المطوّرين الأصليين",
        ],
      },
      ffpkg: { tagline: "مستحسن لـ ShadowMountPlus", cells: ["السرعة الكاملة للصورة", FLAT] },
      exfat: { tagline: "للتوافق", cells: ["السرعة الكاملة للصورة", FLAT] },
      ffpfs: { tagline: "تجريبي", cells: ["السرعة الكاملة للصورة", FLAT] },
      ffpfsc: {
        tagline: "الأصغر، تجريبي",
        cells: ["150–250 ⁦MB/s⁩، وقد يحدث تقطّع", "حسب مستوى الضغط، عادةً أصغر بنسبة 30–60%"],
      },
      folder: { tagline: "ملفات اللعبة كما هي", cells: ["السرعة الكاملة لوحدة التخزين", FLAT] },
    },
    features: [
      ["تحويل", "بين المجلد و ffpkg و exfat و ffpfs و ffpfsc و fpkg، في الاتجاهين."],
      ["نقل", "إرسال الملفات وتحويل الألعاب بين PS5 وأجهزة الكمبيوتر و NAS عبر SMB أو FTP أو NFS."],
      ["تحرير", "ملفات اللعبة كأنها مجلد، واطّلع على غلافها والبرنامج الثابت والباكبورت والمحتوى الإضافي ومشكلات أسماء الملفات قبل البناء."],
      ["بالتدفّق", "من أي مصدر إلى أي وجهة: بلا نسخة مؤقتة، حتى للصورة داخل ffpfsc أو اللعبة داخل أرشيف."],
      ["الأرشيفات", "ملف ⁦.zip⁩ أو ⁦.7z⁩ أو ⁦.rar⁩ يضم لعبة يُتصفَّح ويُحوَّل ويُحزَم كأنه مجلد."],
      ["USB وبطاقات SD", "يُكتب على الذاكرات والبطاقات المهيأة بنظام exFAT أو FAT32."],
      ["الحزم", "حزم أصول zstd أو LZ4 (تجريبي) مع إصلاح ذكي."],
      ["التخزين", "مكتبة لألعابك ومستكشف الملفات لإعادة التسمية والنقل والحذف."],
      ["آمن", "لا تتغير لعبتك أبدًا ما لم تطلب ذلك؛ تُتجاوز الملفات غير المفيدة وتُبلَّغ أسماء الملفات السيئة مسبقًا."],
    ],
    about: "حول",
    compare: "مقارنة الصيغ",
    compareLabel: "مقارنة الصيغ",
    format: "الصيغة",
    credits: "شكر وتقدير",
    upstream: (app) =>
      `أرقام المطوّرين الأصليين: ما يذكره مؤلفو ampr_emu و Lazy_AMPR، ولم يقِسها ${app}؛ وهي تختلف من لعبة إلى أخرى. ampr_emu 0.4.2.1 إصدار تجريبي من المطوّرين الأصليين؛ مشكلة معروفة: تتعطّل بعض الألعاب عند الحفظ.`,
    bundled: "تعمل المجلدات المحزومة بـ LZ4 على ampr_emu 0.4.2.1 من تطوير drakmor (⁦GPL-3.0⁩)، مضمّنًا دون تعديل. الشيفرة المصدرية:",
    lazy: "تعتمد حزم LZ4 على Lazy_AMPR من تطوير Nazky:",
    ng: "تعمل حزم zstd و LZ4 على ampr-emu-ng. المصدر:",
    logo: "الشعار من تصميم Xxcco:",
    unrar:
      "تُقرأ أرشيفات ⁦.rar⁩ بمكتبة UnRAR المضمّنة من تطوير Alexander Roshal (للاستخراج فقط، بموجب ترخيص UnRAR).",
    language: "اللغة",
    system: "لغة النظام",
    notifications: "الإشعار عند اكتمال المهمة",
    notificationsHint: "يظهر على الجهاز لمهام التحويل والحزم ونقل الملفات.",
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
    /** Edit's folds under the game card. */
    facts: "معلومات اللعبة",
    explorer: "ملفات اللعبة",
    howTitle: "كيف يعمل هذا",
    /** The empty state's paragraphs. */
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
    archiveNote: "يمكن تصفّح الأرشيفات لكن لا يمكن تحريرها. حوّله أولًا إلى مجلد أو صورة لتعديله.",
    nasNote: "يمكن تصفح الألعاب الموجودة على NAS واستخراجها، لكن لا يمكن تعديلها. لتغييرها، حوّلها أو استخرجها إلى هذا الجهاز أولًا.",
    patchNote: "هذه حزمة تصحيح: لا تحتوي إلا على ما غيّره التحديث. تظهر الملفات التي تحتويها كاملة؛ يمكن تصفحها واستخراجها، لكن لا يمكن تعديلها.",
    peerNote: "يمكن تصفح الألعاب الموجودة على Forge آخر واستخراجها، لكن لا يمكن تعديلها. لتغييرها، حوّلها أو استخرجها إلى هذا الجهاز أولًا.",
    /** The explorer's changes: none is made until Apply. */
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
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
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
    /** How the changes go in (the pending bar), from core's check before Apply. */
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
    alwaysRebuilt: "يُعاد بناء صور ffpfsc و fpkg دائمًا.",
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
    applyRunning: "جارٍ تطبيق التغييرات. ينتظر التحرير حتى تنتهي المهمة أدناه.",
    applyFailed: "لم تُطبَّق التغييرات:",
    applyRefused: "لا يمكن تطبيق التغييرات بهذه الطريقة:",
    applyCancelled: "أُلغيت المهمة. لا تزال تغييراتك مدرجة هنا.",
    folderPartly: "قد يكون بعضها في المجلد بالفعل: الملفات المعروضة هي ما يحتويه الآن.",
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
      zstdPack: (level: string) => `zstd: حزم · ${level}`,
      level: (n: number) => `المستوى ${num(n)}`,
      zstdUnpack: "zstd: فك الحزم",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
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
    result: (bytes, n) => `${iso(bytes)}، ${count(n, FILES)} ←`,
    outputPath: "مسار الناتج",
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
  // Library.tsx
  library: {
    pickTitle: "اختر مجلدًا لـ «المكتبة»",
    scanningIn: (name) => `جارٍ مسح ${iso(name)}`,
    scanning: "جارٍ المسح",
    cantScanFolder: "تعذّر مسح هذا المجلد",
    chooseAnother: "اختيار مجلد آخر",
    pickPrompt: "اختر مجلدًا يحتوي على مكتبتك لمسحه",
    choose: "اختيار مجلد",
    cantScan: "تعذّر مسح «المكتبة»:",
    cantDelete: "تعذّر الحذف:",
    deleted: (path) => `تم حذف ${iso(path)}`,
    truncated: "بلغ المسح حدّه. قد تكون هذه القائمة غير مكتملة.",
    limits: "يتوقف البحث عند ستة مستويات من المجلدات وبعد نحو 15 ثانية من المسح.",
    noRoots: "لا توجد مجلدات متاحة للمسح.",
    none: "لم يُعثر على ألعاب. يبحث الفحص حتى ستة مستويات من العمق ويتخطى مجلدات النظام. لا يزال بإمكانك اختيار مصادر أخرى في «تحرير» أو «تحويل».",
    skipped: "المواقع والملفات التي تم تخطيها",
    library: "المكتبة",
    storageTip: "المساحة الحرة في التخزين الداخلي لجهاز PS5 (⁦/data⁩)",
    free: (free, total) => (
      <>
        متاح <b>{iso(free)}</b> من {iso(total)}
      </>
    ),
    refresh: "تحديث",
    cards: "عرض البطاقات",
    list: "عرض القائمة",
    games: "الألعاب",
    edit: (n) => `تحرير ${iso(n)}`,
    convert: (n) => `تحويل ${iso(n)}`,
    remove: (n) => `حذف ${iso(n)}`,
    deleting: (n) => `جارٍ حذف ${iso(n)}`,
    lz4Tip: "حزم أصول LZ4",
    zstdTip: "حزم أصول zstd",
    showAs: "طريقة العرض",
    kinds: "عرض النوع",
    all: "الكل",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n, path) => (
      <>
        {n === 1 ? <b>لعبة واحدة</b> : n === 2 ? <b>لعبتان</b> : <><b>{num(n)}</b> {plural(n, { zero: "ألعاب", few: "ألعاب", many: "لعبةً", other: "لعبة" })}</>} في {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "طريقة عرض التخزين",
    library: "المكتبة",
    files: "مستكشف الملفات",
    title: "مستكشف الملفات",
    pickTitle: "اختر مجلدًا لتصفّحه",
    pickPrompt: "اختر مجلدًا لتصفّح ملفاته",
    choose: "اختيار مجلد",
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
    otherDevice: "«النقل» يبقى على جهاز واحد: اسحبه إلى علامة تبويب جهاز آخر لنسخه إليه.",
    dropCopy: (folder) => `أفلِت للنسخ إلى ${iso(folder)}`,
    dropMove: (folder) => `أفلِت للنقل إلى ${iso(folder)}`,
    cantCopy: "تعذّر النسخ:",
    copyNoFolder: "افتح مجلدًا على ذلك الجهاز أولًا.",
    copyOne: (name, to) => `جارٍ نسخ ${iso(name)} إلى ${iso(to)}. يظهر النقل في «الاتصال».`,
    copyMany: (n, to) => `جارٍ نسخ ${count(n, { ...ITEMS, two: "عنصرين" })} إلى ${iso(to)}. يظهر النقل في «الاتصال».`,
    openConnect: "فتح «الاتصال»",
  },
  // Pack.tsx
  pack: {
    title: "الحزم",
    help: "كيف يعمل الحزم",
    engine: "نوع الحزم",
    engines: { zstd: "zstd", lz4: "LZ4" },
    actions: { pack: "حزم", unpack: "فك الحزم" },
    info: (e, packed) =>
      e === "zstd"
        ? packed
          ? "يعيد كتابة حزم zstd كملفات عادية."
          : "يحزم أصول اللعبة في أجزاء zstd."
        : packed
          ? "تجريبي. يعيد كتابة حزم LZ4 كملفات عادية."
          : "تجريبي: قد لا تعمل بعض الألعاب. يحزم أصول اللعبة في أجزاء LZ4.",
    chooseGame: "اختر لعبة أولًا.",
    noAmprGame: (e) => `هذه اللعبة لا تستخدم AMPR، لذا لا يمكن استخدام حزم ${e === "zstd" ? "zstd" : "LZ4"}.`,
    patch: "لا يمكن حزم حزمة تصحيح.",
    additionalContent: "لا يمكن حزم المحتوى الإضافي (بلا ⁦eboot.bin⁩).",
    unreadable: "لا يمكن البدء: تعذّرت قراءة المصدر (راجع «المصدر»).",
    blocked: (a, first, more) => {
      const rest = more > 0 ? ` (بالإضافة إلى ${count(more, { one: "ملاحظة أخرى", two: "ملاحظتين أخريين", few: "ملاحظات أخرى", many: "ملاحظةً أخرى", other: "ملاحظة أخرى" })}، راجع «المصدر»)` : "";
      return a === "pack" ? `لا يمكن الحزم: ${iso(first)}${rest}.` : `لا يمكن فك الحزم: ${iso(first)}${rest}.`;
    },
    target: "الهدف",
    packFolder: (e) => `مستحسن. الأسرع وبأقل تأخير: مجلد لعبة جديد، أصوله في حزم ${e === "zstd" ? "zstd" : "LZ4"}.`,
    levels: { fast: "سريع", medium: "متوسط", max: "أقصى" },
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "مستحسن. zstd 5: سريع، وحجم جيد." },
        medium: { text: "zstd 9: أصغر، وأبطأ." },
        max: { text: "zstd 19: الأصغر؛ أبطأ بكثير، وذاكرة أكثر." },
      },
      lz4: {
        fast: { tone: "good", text: "مستحسن. LZ4 الافتراضي: الأسرع." },
        medium: { text: "LZ4 HC 9: أصغر، وأبطأ." },
        max: { text: "LZ4 HC 12: الأصغر، والأبطأ." },
      },
    },
    runtime: "بيئة التشغيل",
    runtimeNotes: {
      ng: { tone: "good", text: "مستحسن. يقرأ حزم zstd و LZ4." },
      ampr_emu: { text: "⁦ampr_emu⁩ الأصلي." },
    },
    ngNote: `يحتاج إلى ⁦ampr-emu-ng⁩: يثبّته «حزم» في ⁦fakelib/⁩. أوقف تحديث fakelib في ${iso("ShadowMount+")}، وإلا فسيستبدل بيئة التشغيل.`,
    installsRuntime: (version) => `لا يوجد ⁦ampr_emu⁩ هنا: سيثبّت «حزم» ${iso(`ampr_emu ${version}`)}.`,
    freshIndex: (state) =>
      `${state === "stale" ? "⁦ampr_emu.index⁩ قديم" : state === "damaged" ? "⁦ampr_emu.index⁩ تالف" : "لا يوجد ⁦ampr_emu.index⁩"}: سيكتب «حزم» فهرسًا جديدًا.`,
    direct: "يحزم مباشرة إلى الصورة؛ تُقرأ اللعبة مرتين.",
    unpackFolder: "مجلد لعبة جديد بملفات عادية.",
    unpackHint: (n) =>
      n === null
        ? "يعيد كتابة الملفات المحزومة كملفات عادية."
        : `يعيد كتابة ${count(n, { one: "الملف المحزوم الوحيد كملف عادي", two: "الملفين المحزومين كملفين عاديين", few: "ملفات محزومة كملفات عادية", many: "ملفًا محزومًا كملفات عادية", other: "ملف محزوم كملفات عادية" })}.`,
    likelyFail: (fmt) => <>
      من المرجّح أن يفشل البناء بصيغة <b>{fmt}</b>: فهي ترفض أسماء ملفات مدرجة في «المصدر».
    </>,
    collides: (a, folder) =>
      a === "pack"
        ? folder
          ? "هناك مهمة تكتب هذا المجلد بالفعل: يحتاج «حزم» إلى اسم آخر."
          : "هناك مهمة تكتب هذا الملف بالفعل: يحتاج «حزم» إلى اسم آخر."
        : folder
          ? "هناك مهمة تكتب هذا المجلد بالفعل: يحتاج «فك الحزم» إلى اسم آخر."
          : "هناك مهمة تكتب هذا الملف بالفعل: يحتاج «فك الحزم» إلى اسم آخر.",
    already: (a, fmt) => a === "pack"
      ? <>تُكتب هذه اللعبة بالفعل بصيغة <b>{fmt}</b>؛ إعادة «حزم» تُنشئ نسخة ثانية.</>
      : <>تُكتب هذه اللعبة بالفعل بصيغة <b>{fmt}</b>؛ إعادة «فك الحزم» تُنشئ نسخة ثانية.</>,
    emptyTitle: "اختر لعبة تستخدم AMPR",
    experimental: "تجريبي: احتفظ بنسخة من اللعبة الأصلية.",
    source: "المصدر",
  },
  // PackHelp.tsx
  packHelp: {
    title: "كيف يعمل الحزم",
    close: "إغلاق",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>يحزم أصول اللعبة في أجزاء zstd، في مجلد أو صورة. للألعاب التي تستخدم AMPR فقط.</li>
          <li><b>سريع</b> (zstd 5) سريع؛ أما <b>متوسط</b> (9) و<b>أقصى</b> (19) فأصغر وأبطأ.</li>
          <li>تعرض اللعبة المحزومة <b>فك الحزم</b>: فتعود الملفات العادية.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>تجريبي</b>: قد لا تعمل بعض الألعاب. احتفظ بنسخة.</li>
          <li>يحزم في أجزاء LZ4، في مجلد أو صورة: <b>سريع</b> (LZ4)، <b>متوسط</b> (HC 9)، <b>أقصى</b> (HC 12).</li>
          <li>بيئة التشغيل: <b>⁦ampr-emu-ng⁩</b> (الافتراضية) أو <b>⁦ampr_emu⁩</b> الأصلي.</li>
          <li>تعرض اللعبة المحزومة <b>فك الحزم</b>.</li>
        </ol>

        <h3>⁦ampr-emu-ng⁩</h3>
        <ol>
          <li>يقرأ حزم zstd و LZ4؛ ويثبّته «حزم» في ⁦fakelib/⁩.</li>
          <li>أوقف تحديث fakelib في {iso("ShadowMount+")}، وإلا فسيستبدل بيئة التشغيل ولن تتمكن اللعبة من قراءة حزمها.</li>
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
    runtime: {
      forge_release: "النهائية من Forge",
      forge_trace: "بيئة تتبّع LZ4 القديمة",
      zstd: "ampr-emu-ng",
      other: "أخرى",
      none: "لا توجد",
    },
    packed: "محزومة",
    zstdKept: "حزم أصول zstd: يُبقي عليها التحويل",
    damaged: "حزم تالفة",
    plain: "عادية",
    packedDetail: (files, volumes, percent) =>
      `${count(files, FILES)} في ${count(volumes, { one: "جزء واحد", two: "جزأين", few: "أجزاء", many: "جزءًا", other: "جزء" })}` +
      (percent === null ? "" : `، ${num(percent)}% من الحجم`),
    runtimeIs: (label) => `بيئة التشغيل: ${label}`,
    index: {
      stale: "⁦ampr_emu.index⁩ قديم: يحدّثه أي تحويل",
      damaged: "⁦ampr_emu.index⁩ تالف: يعيد أي تحويل كتابته",
      missing: "لا يوجد ⁦ampr_emu.index⁩: يكتبه أي تحويل",
    },
    runtimeAction: (a, version) =>
      a === "install"
        ? `سيثبّت أي تحويل نسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}`
        : a === "replace"
          ? `سيستبدل أي تحويل بيئة التشغيل بنسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}`
          : `⁦ampr_emu⁩ ${iso(version)}: يُبقي عليه أي تحويل`,
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
    wrong: "كلمة المرور غير صحيحة.",
    label: "كلمة المرور",
    cancel: "إلغاء",
    unlock: "فتح القفل",
    checking: "جارٍ التحقق…",
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
    format: {
      folder: { text: "فك صورة أو حزمة وإعادتها إلى مجلد لعبة عادي." },
      exfat: {
        text: "للألعاب التي لا تعمل جيدًا بصيغة ffpkg ولا تعمل إلا كمحتوى قرص خارجي. تُفتح أيضًا على Mac أو PC.",
      },
      ffpkg: {
        text: "نظام الملفات الخاص بـ ⁦PS5 (UFS2)⁩. يمكن تركيبها قابلة للكتابة.",
      },
      ffpfs: {
        text: "نظام الملفات PFS الخاص بـ PS5، دون ضغط. يجب أن تكون أسماء الملفات بأحرف ASCII فقط.",
      },
      ffpfsc: {
        text: "الأصغر: حاوية مضغوطة حول صورة واحدة، تُركّب دائمًا للقراءة فقط.",
      },
      pkg: { tone: "good", text: "مستحسن. تعمل مثل لعبة أصلية، ولكن فقط إن سمح بذلك إصدار البرنامج الثابت لديك." },
      lz4: {
        tone: "warn",
        text: "تجريبي. مجلد لعبة حُزمت أصوله في أجزاء LZ4، يفك محاكي AMPR المضمّن (ampr_emu) حزمها أثناء تشغيل اللعبة. للألعاب التي تستخدم AMPR فقط.",
      },
    },
    innerFfpkg: { text: "نظام الملفات الخاص بـ ⁦PS5 (UFS2)⁩." },
    level0: { text: "دون ضغط: تُخزَّن كل كتلة كما هي. يكون الملف بحجم الصورة." },
    levelDefault: {
      tone: "good",
      text: "مستحسن. هذا هو المستوى الافتراضي في zlib. المستوى 1 يبني أسرع بنحو 3 أضعاف وملفه أكبر بنحو 3%؛ والمستوى 9 أصغر بالكاد.",
    },
    levelLow: {
      text: "يبني أسرع من المستوى 6 (المستوى 1 بنحو 3 أضعاف)؛ لكن الملف يكون أكبر (المستوى 1 بنحو 3%).",
    },
    levelHigh: {
      text: "أصغر بالكاد من المستوى 6 (المستوى 9 بنسبة 0.04%)، لكن الضغط يستغرق وقتًا أطول (المستوى 9 بنحو 25%)، مع انشغال كل الأنوية طوال الوقت.",
    },
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
    separator: "لا يمكن أن يحتوي الاسم على فاصل مجلدات؛ استخدم «تغيير» لاختيار المجلد.",
    badName: "لا يمكن أن يكون الاسم فارغًا أو أن يبدأ بنقطة أو ينتهي بنقطة أو مسافة أو أن يحتوي على محارف تحكم.",
    tooLong: (bytes, max, folder) =>
      folder
        ? `طويل جدًا: ${count(bytes, { one: "بايت واحد", two: "بايتان", few: "بايتات", many: "بايتًا", other: "بايت" })}، والحد الأقصى ${max} (UTF-8).`
        : `طويل جدًا: ${count(bytes, { one: "بايت واحد", two: "بايتان", few: "بايتات", many: "بايتًا", other: "بايت" })} قبل الامتداد، والحد الأقصى ${max} (UTF-8).`,
    pkgExt: (ext) => `ينتهي بـ ${iso(`.${ext}`)}، لكن fpkg يُحفظ بامتداد ${iso(".pkg")}: الامتداد هو ما يحدد برنامج التشغيل.`,
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
    generatePattern: "[اسم اللعبة]-[معرّف اللعبة]",
  },
  // UpdateToast.tsx
  update: {
    available: (v) => `يتوفر ${iso(`v${v}`)}`,
    update: "تحديث",
    later: "لاحقًا",
    updating: "جارٍ التحديث…",
    updatingNote: "يجري تنزيل الإصدار الجديد. سيُعاد تحميل هذه الصفحة بالإصدار الجديد تلقائيًا.",
    downloading: "يجري التنزيل في متصفحك: فُكّ ضغط الأرشيف واستبدل به هذه النسخة.",
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
    details: (name) => `تفاصيل ${iso(name)}`,
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
    pickSendFiles: "الملفات المراد إرسالها",
    pickSendFolder: "المجلد المراد إرساله",
    pickSaveTo: "مكان الحفظ",
    thisBrowser: "هذا المتصفح",
    browserHint: "تذهب الملفات من هذا الهاتف أو الكمبيوتر إلى مجلد على هذا الجهاز، ويحفظ هذا المتصفح ملفًا من هذا الجهاز.",
    uploadTo: "المجلد المراد الرفع إليه",
    uploadPick: "اختيار ملفات للرفع",
    uploading: (name, i, n) => `جارٍ رفع ${iso(name)} (${num(i)} من ${num(n)})`,
    uploaded: "اكتمل الرفع",
    uploadSkipped: (names) => `لم يُرفع، إذ يوجد هناك مجلد أو رابط بالاسم نفسه: ${iso(names)}.`,
    uploadedBefore: (done, n) => `الملفات المرفوعة: ${num(done)} من ${num(n)}. تبقى على هذا الجهاز.`,
    cantStart: "تعذّر بدء النقل:",
    cantUpload: "تعذّر الرفع:",
    pickDownload: "الملف المراد تنزيله",
    transferIntro: "أرسل ملفات أو مجلدًا من أي جهاز إلى مجلد هنا أو على جهاز مقترن أو على NAS. يُنفَّذ النقل كمهمة في قائمة المهام.",
    send: "إرسال",
    sendWhat: "ما المراد إرساله",
    sendTo: "الإرسال إلى",
    sendNeedsDevice: "اتصل بجهاز آخر أولًا: أقرِن Forge أو أضِف NAS.",
    nothingChosen: "لم يُختر شيء بعد",
    chooseFiles: "اختيار ملفات",
    chooseFolder: "اختيار مجلد",
    startTransfer: "بدء",
    uploadHere: "الرفع إلى هذا الجهاز",
    downloadHere: "التنزيل من هذا الجهاز",
    consoleNoFiles: "لا يستطيع متصفح PS5 اختيار الملفات: افتح هذه الصفحة على هاتف أو كمبيوتر.",
    wholeRoot: "لا يمكن إرسال جهاز أو مشاركة أو محرك أقراص بالكامل: اختر مجلدًا داخله.",
    helpPhone: "امسح رمز QR لفتح هذه الصفحة على هاتف. هناك، ينقل «الرفع إلى هذا الجهاز» و«التنزيل من هذا الجهاز» في «نقل الملفات» ملفات الهاتف.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "يربط «الاتصال» نسخ Dump Forge على جهاز PS5 وأجهزة الكمبيوتر لديك في الشبكة نفسها.",
      "للإقران، شغّل «الاكتشاف التلقائي» واختر «إقران» في «بالقرب منك»، أو اكتب عنوانًا في «إضافة جهاز»، ثم اسمح بذلك على الجهاز الآخر. لا تسمح إلا إذا عرضت الشاشتان رقم التحقق نفسه.",
      "يُضاف NAS (SMB وFTP وNFS) بالطريقة نفسها. الأجهزة المقترنة وأجهزة NAS مصادر للألعاب ووجهات للإخراج معًا، ولا تُعدَّل ألعابها هناك.",
      "ينسخ «نقل الملفات» الملفات والمجلدات بين أي منها. ولا يُستبدل أي اسم موجود بالفعل في الوجهة إلا بعد تأكيدك.",
    ],
    helpSecurity: "لا شيء مما يُرسَل بين الأجهزة مشفّر: يمكن لأي شخص على شبكتك قراءته. وتُحفَظ كلمة مرور NAS على هذا الجهاز دون تشفير. استخدم «الاتصال» على شبكتك المنزلية فقط.",
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
    started: (name) => `جارٍ تثبيت ${iso(name)}. تظهر المهمة أدناه.`,
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
    sent: (device) => `يثبّت ${iso(device)} الحزمة. تظهر المهمة أدناه.`,
  },
  duration: { s: (s) => `${s} ث`, ms: (m, s) => `${m} د ${s} ث`, hm: (h, m) => `${h} س ${m} د` },
};

export default ar;
