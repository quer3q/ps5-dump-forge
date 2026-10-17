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
const RESULTS: Noun = { one: "نتيجة واحدة", two: "نتيجتان", few: "نتائج", many: "نتيجةً", other: "نتيجة" };

const ar: Messages = {
  // App.tsx
  app: {
    address: (url) => `العنوان: ${iso(url)}. عرض طريقة الاتصال مع رمز QR`,
    screens: "الأقسام",
    convert: "تحويل",
    active: (n) => `، النشطة: ${num(n)}`,
    edit: "تحرير",
    library: "المكتبة",
    about: "حول",
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
    couldntStop: "تعذّر إيقاف Dump Forge",
    close: "إغلاق",
    tryAgain: "إعادة المحاولة",
    stopped: "توقّف Dump Forge",
    startAgain: (ps5) => (ps5 ? "للمتابعة، افتح لوحة Dump Forge على PS5 مرة أخرى." : "للمتابعة، شغّل أمر ⁦ps5-dump-forge serve⁩ نفسه مرة أخرى."),
    confirmStop: (web: boolean) => (web ? "إيقاف Dump Forge؟" : "إنهاء PS5 Dump Forge؟"),
    stopNote: (web, ps5) => (!web ? "لا توجد مهمة قيد التشغيل." : ps5 ? "لتشغيله مرة أخرى، افتح لوحة Dump Forge على PS5." : "لتشغيله مرة أخرى، شغّل أمر ⁦ps5-dump-forge serve⁩ نفسه من جديد."),
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
      "أصول هذه اللعبة محزومة بـ LZ4، وبيان LZ4 الخاص بها يتضمن الباكبورت: فُكّ حزمها أولًا في تبويب «LZ4»، ثم أزل الباكبورت هنا.",
    unreadable: "لا يمكن البناء: تعذّرت قراءة هذا المصدر (راجع «المصدر»).",
    noContentId: "لا يمكن البناء بصيغة fpkg: لا يحتوي ⁦param.json⁩ لهذه اللعبة على Content ID، وهو ما تحتاجه الحزمة.",
    blocked: (first, more) =>
      `لا يمكن البناء: ${iso(first)}${more > 0 ? ` (بالإضافة إلى ${count(more, { one: "ملاحظة أخرى", two: "ملاحظتين أخريين", few: "ملاحظات أخرى", many: "ملاحظةً أخرى", other: "ملاحظة أخرى" })}، راجع «المصدر»)` : ""}.`,
    amprRuntime: (a, own, version) => {
      if (a === "install") return `تستخدم هذه اللعبة AMPR لكن لا يوجد فيها ⁦ampr_emu⁩: سيثبّت التحويل نسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`;
      if (a === "trace") return `سيستبدل التحويل بيئة تشغيل التتبّع الخاصة بـ LZ4 بنسخة ⁦ampr_emu⁩ النهائية المضمّنة ${iso(version)}.`;
      if (a === "replace")
        return own
          ? `سيستبدل التحويل ⁦ampr_emu⁩ ${iso(own)} الخاص بهذه اللعبة بنسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`
          : `سيستبدل التحويل ⁦ampr_emu⁩ الخاص بهذه اللعبة، الذي لا يذكر رقم إصداره، بنسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`;
      return own
        ? `إصدار ⁦ampr_emu⁩ في هذه اللعبة ${iso(own)} مماثل للنسخة المضمّنة ${iso(version)} أو أحدث منها: سيُبقي عليه التحويل.`
        : `تحتوي هذه اللعبة بالفعل على نسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}: سيُبقي عليها التحويل.`;
    },
    amprIndex: (state, traces) =>
      `${state === "stale" ? "لا يطابق ⁦ampr_emu.index⁩ ملفات هذه اللعبة" : state === "damaged" ? "تتعذّر قراءة ⁦ampr_emu.index⁩" : "لا يوجد ⁦ampr_emu.index⁩ في هذه اللعبة"}: سيكتب التحويل فهرسًا جديدًا${traces ? " ويستبعد بيانات التتبّع القديمة" : ""}.`,
    amprPkgIndex: (traces) =>
      `ستحصل الحزمة على ⁦ampr_emu.index⁩ جديد مبني من ملفاتها${traces ? " دون بيانات التتبّع القديمة" : ""}.`,
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
      ["تحرير", "ملفات اللعبة كأنها مجلد، واطّلع على غلافها والبرنامج الثابت والباكبورت والمحتوى الإضافي ومشكلات أسماء الملفات قبل البناء."],
      ["حذف", "لعبة من القرص بعد تأكيدك."],
      ["LZ4", "تتبّع لعبة تستخدم AMPR ثم حزم أصولها مع إصلاح ذكي (تجريبي)."],
      ["بالتدفّق", "من أي مصدر إلى أي وجهة: بلا نسخة مؤقتة، حتى للصورة داخل ffpfsc أو اللعبة داخل أرشيف."],
      ["الأرشيفات", "ملف ⁦.zip⁩ أو ⁦.7z⁩ أو ⁦.rar⁩ يضم لعبة يُتصفَّح ويُحوَّل ويُحزَم كأنه مجلد."],
      ["المكتبة", "الألعاب على هذا الحاسوب أو الجهاز، جاهزة للتحويل والتحرير."],
      ["آمن", "لا تتغير لعبتك أبدًا ما لم تطلب ذلك؛ تُتجاوز الملفات غير المفيدة وتُبلَّغ أسماء الملفات السيئة مسبقًا."],
      ["USB وبطاقات SD", "يُكتب على الذاكرات والبطاقات المهيأة بنظام exFAT أو FAT32."],
      ["في مكانه أو بإعادة بناء", "تدخل التعديلات مباشرة في الصورة إن كانت تتسع، وإلا في صورة جديدة تحل محلها بعد التحقق."],
    ],
    howToConnect: "طريقة الاتصال",
    connectText: (url) => (
      <>افتح {url} في متصفح على أي جهاز في شبكتك المحلية، أو امسح هذا الرمز.</>
    ),
    close: "إغلاق",
    about: "حول",
    aboutText: "تحوّل هذه الأداة اللعبة من أي صيغة إلى أي صيغة أخرى، على أي منصة، بما فيها PS5.",
    connectAny: "طريقة الاتصال من أي جهاز",
    useAddress: (url) => <>استخدم {url} أو امسح هذا الرمز.</>,
    compare: "مقارنة الصيغ",
    compareLabel: "مقارنة الصيغ",
    format: "الصيغة",
    upstream: (app) =>
      `أرقام المطوّرين الأصليين: ما يذكره مؤلفو ampr_emu و Lazy_AMPR، ولم يقِسها ${app}؛ وهي تختلف من لعبة إلى أخرى. ampr_emu 0.4.2.1 إصدار تجريبي من المطوّرين الأصليين؛ مشكلة معروفة: تتعطّل بعض الألعاب عند الحفظ.`,
    bundled: "تعمل المجلدات المحزومة بـ LZ4 على ampr_emu 0.4.2.1 من تطوير drakmor (⁦GPL-3.0⁩)، مضمّنًا دون تعديل. الشيفرة المصدرية:",
    lazy: "يتبع أسلوب «التتبّع ثم الحزم» مشروع Lazy_AMPR من تطوير Nazky:",
    unrar:
      "تُقرأ أرشيفات ⁦.rar⁩ بمكتبة UnRAR المضمّنة من تطوير Alexander Roshal (للاستخراج فقط، بموجب ترخيص UnRAR).",
    language: "اللغة",
    system: "لغة النظام",
  },
  // Edit.tsx
  edit: {
    details: "التفاصيل",
    deleting: "جارٍ حذف المصدر…",
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
      "افتح **مجلد لعبة** أو **صورتها**، ثم عدّلها كما تعدّل مجلدًا على ذاكرة USB: **أضف الملفات واستبدلها** (اسحبها إلى هنا أو استخدم «إضافة»)، **وأعد التسمية، واحذف**، وأنشئ مجلدات.",
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
    items: (n) => count(n, ITEMS),
    inFolder: (dirs, files) => `${count(dirs, FOLDERS)}، ${count(files, FILES)}`,
    matches: (n) => count(n, RESULTS),
    emptyFolder: "هذا المجلد فارغ.",
    open: (name) => `فتح ${iso(name)}`,
    archiveNote: "يمكن تصفّح الأرشيفات لكن لا يمكن تحريرها. حوّله أولًا إلى مجلد أو صورة لتعديله.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "إضافة ملفات",
    addFolder: "إضافة مجلد",
    pickAddFiles: "اختر ملفات لإضافتها",
    pickAddFolder: "اختر مجلدًا لإضافته",
    nameLabel: "الاسم",
    renameItem: (name) => `إعادة تسمية ${iso(name)}`,
    removeItem: (name) => `حذف ${iso(name)}`,
    renameTip: "إعادة التسمية (F2)",
    removeTip: "حذف (مفتاح Delete)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "استخراج",
    extractItem: (name: string) => `استخراج ${iso(name)}`,
    extractTip: "نسخه إلى مجلد تختاره",
    extractFolder: "استخراج المجلد",
    extractFolderTip: "نسخ المجلد المعروض إلى مجلد تختاره",
    extractGameTip: "نسخ اللعبة كاملة إلى مجلد تختاره",
    pickExtract: "اختر مكان الاستخراج",
    extractPending: "يستخرج الملفات كما هي الآن، دون تغييراتك المعلّقة",
    nameEmpty: "اكتب اسمًا.",
    nameBad: `لا يمكن أن يحتوي الاسم على ${iso("/")} أو ${iso("\\")}، ولا أن يكون ${iso(".")} أو ${iso("..")}`,
    nameTaken: (name) => `${iso(name)} موجود هنا بالفعل.`,
    markNew: "جديد",
    markReplaced: "مُستبدَل",
    markRenamed: "أُعيدت تسميته",
    wasAt: (path) => `كان في ${iso(path)}`,
    sizeLater: "أكثر مما أمكن سرده: يُقرأ الباقي عند تطبيق التغييرات",
    dropInto: (folder) => `أفلِت للإضافة إلى ${iso(folder)}`,
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
    undoTip: "التراجع عن آخر تغيير",
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
    lz4: {
      pack: "LZ4: حزم",
      unpack: "LZ4: فك الحزم",
      trace: "LZ4: تتبّع",
      unpatch: "LZ4: إلغاء الترقيع",
      runtime: "بيئة تشغيل AMPR",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "تحرير",
    editRebuild: "تحرير · إعادة بناء",
    extract: "استخراج",
    job: (id) => `المهمة ${id}`,
    done: "اكتملت",
    cancelled: "أُلغيت",
    failed: "فشلت",
    cancelling: "جارٍ الإلغاء…",
    queued: "في الانتظار",
    dismiss: "إخفاء",
    stopJob: "إيقاف المهمة؟",
    cancel: "إلغاء",
    stopHint: "انقر «إيقاف المهمة؟» مرة أخرى خلال 4 ثوانٍ لإيقافها.",
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
    stays: "تبقى النتائج هنا حتى تضغط «تحديث». تظهر النواتج الجديدة بعد التحديث.",
    library: "المكتبة",
    storageTip: "المساحة الحرة في التخزين الداخلي لجهاز PS5 (⁦/data⁩)، وقت آخر مسح",
    free: (free, total) => (
      <>
        متاح <b>{iso(free)}</b> من {iso(total)}
      </>
    ),
    refresh: "تحديث",
    cards: "عرض البطاقات",
    list: "عرض القائمة",
    games: "الألعاب",
    edit: (n) => `تحرير ${n}`,
    convert: (n) => `تحويل ${n}`,
    remove: (n) => `حذف ${n}`,
    lz4Tip: "محزومة بـ LZ4: حزم أصول ampr_emu",
    lz4Packed: " محزومة",
  },
  // Lz4.tsx
  lz4: {
    title: "حزم أصول LZ4",
    help: "كيف يعمل LZ4",
    scenario: "سيناريو LZ4",
    scenarios: { trace: "التتبّع", pack: "الحزم/فك الحزم" },
    actions: { patch: "ترقيع", unpatch: "إلغاء الترقيع", pack: "حزم", unpack: "فك الحزم" },
    scenarioInfo: (s, packed) => s === "trace"
      ? "يسجّل الأصول التي تقرؤها اللعبة أثناء اللعب ليستخدمها «حزم»."
      : packed
        ? "يعيد كتابة حزم LZ4 الخاصة باللعبة كملفات عادية."
        : "يحزم أصول اللعبة في أجزاء LZ4 يقرؤها ampr_emu أثناء تشغيل اللعبة.",
    traceInfo: (a, image) => a === "patch"
      ? image
        ? "يعيد بناء هذه الصورة مع بيئة تشغيل التتبّع، ثم يستبدلها."
        : "يثبّت بيئة تشغيل التتبّع في هذا المجلد."
      : image
        ? "يعيد بناء هذه الصورة مع بيئة التشغيل النهائية، ثم يستبدلها."
        : "يعيد بيئة التشغيل النهائية إلى هذا المجلد.",
    noAmpr: "اللعبة لا تستخدم AMPR",
    noEboot: "لا يوجد ⁦eboot.bin⁩",
    packedFirst: "المصدر محزوم بـ LZ4: فُكّ حزمه أولًا",
    damaged: "حزم LZ4 الخاصة به تالفة",
    archive: "لا يمكن تتبّع أرشيف: حوّله أولًا",
    chooseFirst: "اختر مصدرًا أولًا",
    notRead: "لم تتم قراءة المصدر بعد",
    pickProfile: "ملف تعريف LZ4",
    profileFilter: "ملف تعريف ⁦LZ4 (TOML)⁩",
    pickTracesFolder: "بيانات تتبّع LZ4 (مجلد يحتوي على ⁦ampr_commands.bin⁩ و ⁦ampr_emu.index⁩)",
    pickTracesEither: "بيانات تتبّع LZ4: ملف zip، أو مجلد يحتوي على الملفين",
    pickTracesZip: "بيانات تتبّع ⁦LZ4 (zip)⁩",
    unreadable: "لا يمكن البدء: تعذّرت قراءة هذا المصدر (راجع «المصدر»).",
    pending: (a) => a === "patch"
      ? "لا يمكن الترقيع أثناء وجود مهام قيد التشغيل أو في الانتظار."
      : "لا يمكن إلغاء الترقيع أثناء وجود مهام قيد التشغيل أو في الانتظار.",
    replacing: "هناك مهمة تعيد بناء هذه الصورة بالفعل: انتظر حتى تنتهي.",
    blocked: (a, first, more) => {
      const rest = more > 0 ? ` (بالإضافة إلى ${count(more, { one: "ملاحظة أخرى", two: "ملاحظتين أخريين", few: "ملاحظات أخرى", many: "ملاحظةً أخرى", other: "ملاحظة أخرى" })}، راجع «المصدر»)` : "";
      if (a === "patch") return `لا يمكن الترقيع: ${iso(first)}${rest}.`;
      if (a === "unpatch") return `لا يمكن إلغاء الترقيع: ${iso(first)}${rest}.`;
      if (a === "pack") return `لا يمكن الحزم: ${iso(first)}${rest}.`;
      return `لا يمكن فك الحزم: ${iso(first)}${rest}.`;
    },
    chooseGame: "اختر مجلد لعبة أو صورة أولًا.",
    noAmprGame: "هذه اللعبة لا تستخدم AMPR، لذا لا يمكن استخدام LZ4 هنا.",
    additionalContent: "هذا محتوى إضافي (بلا ⁦eboot.bin⁩)، لذا لا يمكن استخدام LZ4 هنا.",
    installsRuntime: (a, version) =>
      a === "pack"
        ? `لا تحتوي هذه اللعبة على ⁦ampr_emu⁩، لذا سيثبّت الحزم نسخة ⁦ampr_emu⁩ المضمّنة ${iso(version)}.`
        : a === "patch"
          ? `لا تحتوي هذه اللعبة على ⁦ampr_emu⁩، لذا سيثبّت الترقيع بيئة تشغيل التتبّع المضمّنة (${iso(`ampr_emu ${version}`)}).`
          : `لا تحتوي هذه اللعبة على ⁦ampr_emu⁩، لذا سيثبّت إلغاء الترقيع بيئة التشغيل النهائية المضمّنة (${iso(`ampr_emu ${version}`)}).`,
    freshIndex: (state, a) =>
      `${state === "stale" ? "لا يطابق ⁦ampr_emu.index⁩ ملفات هذه اللعبة" : state === "damaged" ? "تتعذّر قراءة ⁦ampr_emu.index⁩" : "لا يوجد ⁦ampr_emu.index⁩ في هذه اللعبة"}: سيكتب ${a === "pack" ? "الحزم" : a === "patch" ? "الترقيع" : "إلغاء الترقيع"} فهرسًا جديدًا.`,
    unavailable: (items) => `غير متاح لهذا المصدر: ${items.join("؛ ")}.`,
    readOnly: (kind) => <>
      صيغة <b>{kind}</b> للقراءة فقط على جهاز PS5، لذا لا يمكن تتبّعها. حوّلها إلى <b>ffpkg</b> أو <b>exfat</b> أولًا.
    </>,
    action: "الإجراء",
    traceAction: "إجراء التتبّع",
    changes: (a, image) => a === "patch"
      ? image
        ? "يغيّر هذه الصورة: تحل بيئة تشغيل التتبّع محل ⁦fakelib/libSceAmpr.sprx⁩، دون نسخة احتياطية."
        : "يغيّر هذا المجلد: تحل بيئة تشغيل التتبّع محل ⁦fakelib/libSceAmpr.sprx⁩، دون نسخة احتياطية."
      : image
        ? "يغيّر هذه الصورة: تعود بيئة التشغيل النهائية وتُحذف بيانات التتبّع، دون نسخة احتياطية."
        : "يغيّر هذا المجلد: تعود بيئة التشغيل النهائية وتُحذف بيانات التتبّع، دون نسخة احتياطية.",
    replay: (web) => web
      ? "العب، ثم اختر هذا المصدر مرة أخرى لاستخدام بيانات التتبّع الخاصة به. ويتيحها «التتبّع» أيضًا كملف zip."
      : "العب، ثم اختر هذا المصدر مرة أخرى لاستخدام بيانات التتبّع الخاصة به.",
    room: "مساحة لبيانات التتبّع",
    roomHint: "مساحة حرة لبيانات تتبّع اللعبة. ركّب الصورة باستخدام ⁦image_rw=⁩، وإلا فلن يُسجَّل شيء.",
    alreadyRelease: "بيئة التشغيل النهائية موجودة بالفعل: ستُعاد كتابة الفهرس فقط.",
    spaceHint: "يحتاج إلى مساحة حرة بحجم الصورة تقريبًا؛ لا تُستبدل الصورة إلا بعد التحقق من النسخة.",
    deletesTraces: (a, bytes, web) =>
      a === "patch"
        ? web
          ? `يحذف «ترقيع» بيانات تتبّع هذا المصدر (${bytes}): نزّلها أولًا.`
          : `يحذف «ترقيع» بيانات تتبّع هذا المصدر (${bytes}): انسخها أولًا.`
        : web
          ? `يحذف «إلغاء الترقيع» بيانات تتبّع هذا المصدر (${bytes}): نزّلها أولًا.`
          : `يحذف «إلغاء الترقيع» بيانات تتبّع هذا المصدر (${bytes}): انسخها أولًا.`,
    useProfile: "استخدام ملف تعريف قواعد بدلًا منها",
    profile: "ملف التعريف",
    choose: "اختيار",
    profileHint: "ملف TOML بقواعد الحزم، يُستخدم بدلًا من بيانات التتبّع.",
    target: "الهدف",
    packInto: "الحزم إلى",
    direct: "يحزم مباشرة إلى الصورة؛ تُقرأ ملفات اللعبة مرتين.",
    targetFormat: "الصيغة الهدف",
    unpackFolder: "مجلد لعبة جديد تُعاد فيه كتابة الحزم كملفات عادية.",
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
    profileRules: "يحزم ما تحدده قواعد ملف التعريف؛ وتبقى الملفات التي يفتحها المحرك مباشرة غير محزومة.",
    chosenRules: "يحزم الملفات التي قرأتها اللعبة في بيانات التتبّع هذه؛ وتبقى الملفات التي يفتحها المحرك مباشرة غير محزومة.",
    ownRules: "يحزم الملفات التي قرأتها اللعبة في آخر جلسة تتبّع لها؛ وتبقى الملفات التي يفتحها المحرك مباشرة غير محزومة.",
    guessRules: "لا توجد بيانات تتبّع: يحزم وفق تخمين مدمج أقل موثوقية. نفّذ التتبّع أولًا، أو اختر بيانات تتبّع أو ملف تعريف.",
    recorded: "بيانات التتبّع المسجّلة",
    recordedHint: (bytes) => `⁦ampr_commands.bin⁩ (${iso(bytes)}): يستخدم «حزم» بيانات التتبّع هذه.`,
    downloadHint: "أغلق اللعبة وانتظر دقيقة، ثم نزّل من متصفح كمبيوتر. واختر ملف zip هناك في «الحزم/فك الحزم» ← «بيانات التتبّع».",
    download: (bytes, zip) => `تنزيل بيانات التتبّع (${bytes}) باسم ${iso(zip)}`,
    missingIndex: "لا يوجد ⁦ampr_emu.index⁩ في هذا المصدر: يحتاج «حزم» إليه بجوار ⁦ampr_commands.bin⁩.",
    truncatedJournal: "يُحزم سجل التتبّع المقطوع في نهايته أيضًا، حتى آخر إدخال مكتمل فيه.",
    fakelibWarning: (recorded) => recorded
      ? "بيانات التتبّع قليلة أو معدومة؟ أوقف أداة تحديث fakelib، ثم نفّذ «ترقيع» مجددًا والعب مرة أخرى."
      : "أوقف أداة تحديث fakelib أثناء التتبّع، وإلا فلن يُسجَّل شيء.",
    emptyTitle: "اختر لعبة تستخدم AMPR",
    experimental: "تجريبي: احتفظ بنسخة من اللعبة الأصلية.",
    chooseSource: "اختيار المصدر",
    source: "المصدر",
    traces: "بيانات التتبّع",
    optionalTraces: "بيانات التتبّع (اختياري)",
    unlock: "إلغاء القفل لاختيار بيانات تتبّع أخرى",
    loaded: "بيانات التتبّع محمّلة بالفعل من هذا المصدر",
    lockedHint: (bytes) => `بيانات تتبّع هذا المصدر نفسه (${iso(bytes)}). انقر القفل لاختيار غيرها.`,
    clear: "إزالة",
    zip: "Zip",
    folder: "مجلد",
    tracesHint: "اختر ملف ⁦amprtrace⁩ بصيغة zip من «تنزيل بيانات التتبّع»، أو مجلدًا يحتوي على ملفي التتبّع.",
    useOwn: "استخدام بيانات تتبّع المصدر",
    patchResult: (a, n) => a === "patch"
      ? `تم الترقيع: بيئة تشغيل التتبّع مثبّتة؛ ويسرد الفهرس ${count(n, FILES_GEN)}.`
      : `تم إلغاء الترقيع: عادت بيئة التشغيل النهائية؛ ويسرد الفهرس ${count(n, FILES_GEN)}.`,
    removed: (files) => `تم حذف ملفات التتبّع القديمة: ${files.map(iso).join("، ")}.`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "كيف يعمل LZ4",
    close: "إغلاق",
    warning:
      "تجريبي، لألعاب AMPR فقط. احتفظ بنسخة من اللعبة الأصلية.",
    body: () => (
      <>
        <h3>1. الحزم/فك الحزم</h3>
        <ol>
          <li>
            اختر اللعبة الأصلية، ثم <b>الحزم/فك الحزم</b>، ثم بيانات التتبّع (أو ملف تعريف).
          </li>
          <li>أنشئ مجلدًا محزومًا أو صورة عادية؛ ويمكن تحويل المجلد إلى أي صيغة في <b>تحويل</b>.</li>
          <li>
            يعرض المصدر المحزوم <b>فك الحزم</b> بدلًا من ذلك: فتعود الملفات غير المحزومة.
          </li>
          <li>يُبقي الحزم على ⁦ampr_emu⁩ الخاص باللعبة إذا كان بالإصدار المضمّن أو أحدث ويقرأ حزم LZ4؛ وإلا فيثبّت النسخة النهائية المضمّنة، حتى بدلًا من بيئة تشغيل التتبّع الخاصة بـ LZ4 وفي لعبة لا تحتوي عليها.</li>
        </ol>

        <h3>2. التتبّع</h3>
        <ol>
          <li>
            اختر ملف <b>ffpkg</b> أو <b>exfat</b> الخاص باللعبة، ثم <b>التتبّع</b> ← <b>ترقيع</b>.
          </li>
          <li>أوقف أداة تحديث fakelib: فهي تستبدل بيئة تشغيل التتبّع.</li>
          <li>
            ركّب الصورة باستخدام <span className="path">image_rw=</span>، والعب جلسة واحدة، ثم أغلق اللعبة.
          </li>
          <li>
            استخدم <b>تنزيل بيانات التتبّع</b> من صفحة PS5، أو انسخ ⁦ampr_commands.bin⁩ و ⁦ampr_emu.index⁩.
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
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
    chooseFiles: (n) => `اختيار ${count(n, FILES_GEN)}`,
    nothing: "لا توجد هنا مجلدات أو ملفات.",
    cancel: "إلغاء",
    chooseFolder: "اختيار هذا المجلد",
  },
  // common.tsx
  common: {
    folder: "مجلد",
    pickFolder: "مجلد اللعبة",
    pickImage: "صورة اللعبة أو أرشيفها",
    pickImageFilter: "الصور والأرشيفات",
    pickFolderButton: "مجلد",
    pickImageButton: "صورة/أرشيف",
    source: "المصدر",
    emptyTitle: "اختر مجلد لعبة أو صورة أو أرشيفًا",
    emptyText: "مجلد لعبة PS5 أو صورة أو حزمة أو أرشيف منه.",
    accepted: "المصادر المقبولة",
    unknownTitle: "عنوان غير معروف",
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
      forge_trace: "تتبّع Forge",
      other: "أخرى",
      none: "لا توجد",
    },
    packed: "محزومة",
    damaged: "حزم تالفة",
    traced: "متتبَّعة",
    plain: "عادية",
    packedDetail: (files, volumes, percent) =>
      `${count(files, FILES)} في ${count(volumes, { one: "جزء واحد", two: "جزأين", few: "أجزاء", many: "جزءًا", other: "جزء" })}` +
      (percent === null ? "" : `، ${num(percent)}% من الحجم`),
    journal: (size) => `سجل التتبّع ${size}`,
    noJournal: "لا يوجد سجل تتبّع بعد",
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
        : "سيُحذف ملف الصورة نهائيًا: لا سلة مهملات ولا تراجع. إذا توقف الحذف في منتصفه، فقد يكون جزء منه قد حُذف بالفعل.",
    no: "لا",
    yes: "نعم، احذف",
    archiveTitle: "حذف هذا الأرشيف؟",
    archiveText:
      "سيُحذف ملف الأرشيف نهائيًا: لا سلة مهملات ولا تراجع. تبقى الأجزاء الأخرى من الأرشيف متعدد الأجزاء كما هي.",
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
        tone: "good",
        text: "مستحسن. نظام الملفات الخاص بـ ⁦PS5 (UFS2)⁩. يمكن تركيبها قابلة للكتابة.",
      },
      ffpfs: {
        tone: "warn",
        text: "تجريبي. نظام الملفات PFS الخاص بـ PS5، دون ضغط. يجب أن تكون أسماء الملفات بأحرف ASCII فقط.",
      },
      ffpfsc: {
        tone: "warn",
        text: "تجريبي. الأصغر: حاوية مضغوطة حول صورة واحدة، تُركّب دائمًا للقراءة فقط.",
      },
      pkg: {
        text: "تُثبَّت مثل لعبة من المتجر وتعمل بالسرعة الأصلية. تحتاج إلى kstuff و fpkg-enable و ppr-patch.",
      },
      lz4: {
        tone: "warn",
        text: "تجريبي. مجلد لعبة حُزمت أصوله في أجزاء LZ4، يفك محاكي AMPR المضمّن (ampr_emu) حزمها أثناء تشغيل اللعبة. للألعاب التي تستخدم AMPR فقط.",
      },
    },
    innerFfpkg: { tone: "good", text: "مستحسن. نظام الملفات الخاص بـ ⁦PS5 (UFS2)⁩." },
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
    pkgFirmware: "تعمل فقط على البرنامج الثابت 11.60 أو أقدم.",
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
    updatingNote: "يجري تنزيل الإصدار الجديد، وسيُعاد تشغيل التطبيق تلقائيًا.",
    downloading: "يجري التنزيل في متصفحك: فُكّ ضغط الأرشيف واستبدل به هذه النسخة.",
    failed: (v) => `تعذّر التحديث إلى ${iso(`v${v}`)}`,
    timeout: (v) => `لم يبدأ الإصدار ${iso(`v${v}`)} خلال دقيقة. إذا اكتمل تنزيله، فأوقف Dump Forge وافتحه من جديد لإكمال التحديث.`,
    retry: "إعادة المحاولة",
    close: "إغلاق",
  },
  duration: { s: (s) => `${s} ث`, ms: (m, s) => `${m} د ${s} ث`, hm: (h, m) => `${h} س ${m} د` },
};

export default ar;
