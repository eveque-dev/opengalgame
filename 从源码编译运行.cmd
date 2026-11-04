@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"
set "OPENGALGAME_CARGO=cargo"
where cargo >nul 2>&1
if errorlevel 1 (
  if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
    set "OPENGALGAME_CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
  ) else (
    echo 未找到 Rust。请先安装 Rust MSVC 工具链及 Visual Studio C++ 构建工具。
    pause
    exit /b 1
  )
)
if /i "%~1"=="--check" (
  "%OPENGALGAME_CARGO%" --version
  exit /b %errorlevel%
)
echo 正在从当前源码编译OpenGalgame，首次运行需要下载依赖，请保持窗口打开。
"%OPENGALGAME_CARGO%" build --release --locked
if errorlevel 1 (
  echo 编译失败。请查看上方错误；若程序文件被占用，请关闭从源码启动的编辑器后重试。
  pause
  exit /b 1
)
start "" /D "%~dp0" "%~dp0target\release\opengalgame.exe" --hub
exit /b 0
