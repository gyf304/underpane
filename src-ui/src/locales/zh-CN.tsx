const messages = {
  "status.idle": "无更改",
  "status.unsaved": "未保存的更改",
  "status.saving": "保存中…",
  "status.saved": "已保存",
  "status.error": "保存失败",

  "editor.loading": "加载中…",
  "editor.monitors": "显示器",
  "editor.generalSettings": "常规设置",
  "editor.save": "保存",
  "editor.selectMonitor": "请选择一个显示器进行配置。",
  "editor.wallpaper": "壁纸",
  "editor.none": "无",
  "editor.noFields": "没有可配置的字段。",
  "editor.chooseFile": "选择文件…",
  "editor.noFileChosen": "未选择文件",
  "editor.chooseFolder": "选择文件夹…",
  "editor.noFolderChosen": "未选择文件夹",

  "general.wallpapersDir.label": "壁纸目录",
  "general.wallpapersDir.desc": "从该文件系统路径加载壁纸。",
  "general.configFile.label": "配置文件",
  "general.configFile.desc": "在默认编辑器中打开配置文件。",
  "general.configFile.button": "打开配置文件",
  "general.wallpapersFolder.label": "壁纸文件夹",
  "general.wallpapersFolder.desc": "在文件浏览器中打开壁纸目录。",
  "general.wallpapersFolder.button": "打开壁纸文件夹",

  "general.autostart.label": "登录时启动",
  "general.autostart.desc": "登录时自动启动 Underpane。",

  "general.quickstart.label": "快速入门",
  "general.quickstart.desc": "重新播放欢迎导览。",
  "general.quickstart.button": "显示快速入门",

  "quickstart.skip": "跳过",
  "quickstart.back": "上一步",
  "quickstart.next": "下一步",
  "quickstart.done": "完成",

  "quickstart.welcome.title": "欢迎使用 Underpane",
  "quickstart.welcome.desc":
    "Underpane 可以为每台显示器分别设置动态壁纸。下面带您快速熟悉一下主要功能。",
  "quickstart.monitors.title": "1. 选择显示器",
  "quickstart.monitors.desc":
    "在左侧面板点击一台显示器即可进行设置。面板顶部的示意图对应您的实际显示器布局，可供参考。选择「*」可为所有显示器统一应用同一张壁纸。",
  "quickstart.wallpaper.title": "2. 选择壁纸",
  "quickstart.wallpaper.desc":
    "选中显示器后，在下拉菜单中选择一张壁纸；如需清除，选择「无」即可。",
  "quickstart.config.title": "3. 调整设置",
  "quickstart.config.desc":
    "每张壁纸都有各自的选项，包括开关、滑块和输入框，并按类别分组。点击右上角工具栏的「保存」即可应用更改。",
  "quickstart.addWallpapers.title": "4. 添加更多壁纸",
  "quickstart.addWallpapers.desc":
    "打开左下角的「常规设置」，点击「打开壁纸文件夹」，将新的壁纸文件夹放入其中即可安装。",
  "quickstart.autostart.title": "5. 登录时启动",
  "quickstart.autostart.desc":
    "在「常规设置」中开启「登录时启动」，Underpane 便会随系统自动运行；完成后点击「保存」以应用。",
  "quickstart.done.title": "一切就绪",
  "quickstart.done.desc":
    "以上就是全部内容，欢迎使用 Underpane！您随时可以在「常规设置」中重新查看本快速入门。",

  "install.title": "安装壁纸",
  "install.from": ({ url }: { url: string }) => (
    <>
      来源：<code className="text-xs">{url}</code>
    </>
  ),
  "install.folder.label": "安装到文件夹",
  "install.error.nameRequired": "请输入名称",
  "install.error.nameInvalid":
    "仅允许小写字母、数字和 '-'（不能以 '-' 开头或结尾）",
  "install.cancel": "取消",
  "install.install": "安装",
  "install.installing": ({ name }: { name: string }) => (
    <>
      正在安装 <b>{name}</b>…
    </>
  ),
  "install.validating": "验证中…",
  "install.installed": ({ name }: { name: string }) => (
    <>
      已安装 <b>{name}</b>。
    </>
  ),
  "install.close": "关闭",
  "install.retry": "重试",
  "install.filesSuffix": ({ done, total }: { done: number; total: number }) =>
    ` · ${done}/${total} 个文件`,
};

export default messages;
