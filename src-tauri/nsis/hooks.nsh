; 安装包的自定义设置，由 tauri.conf.json 的 bundle.windows.nsis.installerHooks 引入。
; 本文件在安装包页面定义之前引入，因此可以设置完成页的选项。

; 完成页的“创建桌面快捷方式”默认不勾选：托盘程序平时从托盘和开始菜单打开，用不到桌面快捷方式。
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED
