---
title: 平台概览
description: "NSH 在哪些平台上运行：iOS、Android、OHOS、macOS、Windows、Linux、GPUI 和 Web，以及各平台的后端、工具和嵌入流程。"
order: 10
group: platforms
---

# 平台概览

Native Shell 运行在手机、桌面和网页上：iOS、Android、OHOS、macOS、Windows、Linux、GPUI 和 Web。各后端使用平台自己的界面技术绘制：iOS 的 UIKit、OHOS 的 ArkUI、macOS 的 AppKit、Linux 的 GTK 4、GPUI 应用中的 GPUI 元素，以及浏览器中的 HTML。QuickJS、响应式图、渲染器、Taffy 布局和组件共享 Rust 代码；各平台指南说明对应后端提供的界面能力和宿主 API。

NSH 的主要用途是把模块嵌入你自己的原生应用，由它在运行时加载。CLI 开发宿主使用热更新（`nsh run`），应用 runner 将模块打包成可安装应用（`nsh package`）。OHOS 使用 [DevEco 嵌入流程](./embedding-ohos.md)，模块源码随宿主 HAP 发布。

## 一览

| 平台 | 后端；文本 | 运行环境 | 开发 | 打包 |
|---|---|---|---|---|
| [iOS](./embedding-ios.md) | UIKit；CoreText | iOS 15.0+，真机和模拟器 | `nsh run --target ios` | `.app`（模拟器）、签名的 `.ipa` |
| [Android](./embedding-android.md) | Android 视图；`StaticLayout` | Android 9（API 28）+，`arm64-v8a` 和 `x86_64` | `nsh run --target android` | 签名的 `.apk` |
| [OHOS](./embedding-ohos.md) | ArkUI 自定义节点；原生文本排版 | API 12 基线，arm64 | `cargo xtask ohos`、DevEco 宿主 | 用 DevEco 构建宿主 HAP |
| [macOS](./macos.md) | AppKit 与 Core Animation；CoreText | macOS 14+ | `nsh run --target macos` | ad hoc 签名的 `.app` |
| [Windows](./windows.md) | 用 Direct2D 绘制的 Win32 窗口；DirectWrite | Windows 10 1809+、Windows 11 | `nsh run`（Windows 上的默认目标） | 一个目录和一个 `.zip` |
| [Linux](./linux.md) | GTK 4 控件；Pango | GTK 4.14+ | `nsh run`（Linux 上的默认目标） | 一个目录和一个 `.tar.gz` |
| [GPUI](./gpui.md) | GPUI；GPUI 的文字系统 | macOS、Windows、Linux（Vulkan） | `nsh run --target gpui` | 与所在桌面平台相同 |
| [Web](./web.md) | HTML 与 CSS；浏览器的字体 | 现代浏览器，桌面和手机 | `nsh run --target web` | 一个静态网站 |

- **[GPUI](./gpui.md)** 用姊妹项目 GPUI Kit 所基于的 [GPUI](https://gpui-kit.com) 绘制模块，在 macOS、Windows 和 Linux 上表现一致：一套渲染覆盖三个桌面平台，`AppView` 可以放进 GPUI 和 GPUI Kit 应用自己的元素树。
- **[Web](./web.md)** 把应用渲染成浏览器页面里的 HTML：`nsh package --target web` 发布一个静态网站，可部署到任意 Web 服务器或 CDN；你自己的网页可以用 `createPlayer` 或 iframe 嵌入应用。本站的在线示例和 [Playground](/playground) 也由它运行。
- **Mac Catalyst** 在 Mac 上用 iPhone 尺寸的窗口（390×844）运行 iOS 后端，用于开发。它是 macOS 上 `nsh run` 的默认目标：最快看到模块在手机上的样子。

## 开发与打包

不传 `--target` 时，`nsh run` 选择本机的平台：macOS 上是 Mac Catalyst，Linux 上是 GTK 窗口，Windows 上是 Win32 窗口。手机要从电脑上连接：iOS 需要装有 Xcode 的 Mac，Android 可以在装有 Android SDK 的 macOS、Linux 或 Windows 上开发（[安装](./installation.md#各平台可用的命令)）。

```sh
nsh run my-app --target macos      # 或 ios、android、windows、linux、gpui、web、catalyst
nsh package my-app --target macos  # 或 ios、ios-simulator、android、windows、linux、gpui、web
```

桌面后端只为运行它的机器构建：Mac 应用在 Mac 上打包，Windows 应用在 Windows 上打包，Linux 应用在 Linux 上打包。各目标的输出、签名和图标见[打包](./packaging.md)。

## 嵌入原生应用

| 平台 | 宿主视图 | 发布形式 | 宿主语言 |
|---|---|---|---|
| iOS | `NshAppView`（一个 `UIView`） | `NativeShell.xcframework` | Swift |
| Android | `NshView`（一个 `FrameLayout`） | `nsh.aar` | Java 或 Kotlin |
| [OHOS](./embedding-ohos.md) | ArkTS `NshView` 中的 `NshSurface` | 动态库和自动生成的声明 | ArkTS |
| macOS | `AppView`（一个 `NSView`） | `nsh-platform-macos` crate | Rust |
| Windows | `AppView`（一个子窗口） | `nsh-platform-windows` crate | Rust |
| Linux | `AppView`（一个 GTK 控件） | `nsh-platform-linux` crate | Rust |
| GPUI（macOS、Windows、Linux） | `AppView`（一个 GPUI 视图） | `nsh-platform-gpui` crate | Rust |
| Web | `createPlayer`（一个页面元素）或 iframe | `nsh package --target web` 生成的运行时文件 | JavaScript 或 HTML |

每个视图都是一个拥有独立虚拟机的引擎，一个应用可以同时显示多个。[原生组件指南](./native-components.md)说明各 SDK 的宿主视图注册 API。

> [!NOTE]
> 桌面宿主通过 NSH 仓库里的 Rust crate 嵌入 NSH；它们的 `AppView` 从应用自己的文件（或源代码）加载 bundle，使用默认限制。iOS 和 Android SDK 还能用 `NshTrust` 加载[签名包](./signed-packages.md)，并为每个视图设置限制和能力（[沙箱与能力](./sandbox-and-capabilities.md)）。

## 为每个平台编写

应用的大部分代码不需要判断平台：元素、样式、布局和模块在各处行为一致。平台确实不同的地方，NSH 给它一个固定的位置（[平台特定代码](./platform-specific.md)）：

- **平台专属属性**放在 `ios={{…}}`、`android={{…}}`、`web={{…}}` 或 `desktop={{…}}`（macOS、Windows、Linux 和桌面浏览器）里。
- **`Platform.select`** 按操作系统取值，也可以按 `mobile` 和 `desktop` 整体取值；`Platform.OS` 是 `"ios"`、`"android"`、`"ohos"`、`"macos"`、`"windows"`、`"linux"` 或 `"web"`。
- **输入跟随平台。** 手机上是触摸和软键盘；桌面上是鼠标、触控板和键盘。

[`nsh/ui/mobile`](../components/index.md) 的组件由 NSH 绘制，所以在桌面上也能运行，样子和在手机上一样。桌面布局用基础元素（`View`、`Text`、`Scroll`、`List` 等）和 `Platform.select({ desktop: … })` 搭建，示例应用 gallery 的桌面布局就是这样做的；桌面 UI 套件也在路上。

每个后端都列出了它尚不支持的内容：[macOS](./macos.md#macos-上的差异)、[Windows](./windows.md#windows-上的差异)、[Linux](./linux.md#linux-上的差异)、[GPUI](./gpui.md#差异)。

## 另请参阅

- [快速开始](./getting-started.md)：第一个模块，在每个平台上运行。
- [安装](./installation.md)：每个平台需要的工具。
- [架构](./architecture.md)：哪些部分共享，每个后端做什么。
