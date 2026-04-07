@echo off
set HTTPS_PROXY=
set HTTP_PROXY=
set GOOGLE_APPLICATION_CREDENTIALS=C:\Users\at384\Downloads\osc\dbg-grcit-dev-e1-c79e5571a5a7.json
set RUST_LOG=openfang_runtime::drivers::vertex=debug,openfang=info
set RUST_BACKTRACE=full
cd /d C:\Users\at384\Downloads\osc\dllm\openfang
echo Getting access token...
for /f "tokens=*" %%a in ('gcloud auth print-access-token') do set VERTEX_AI_ACCESS_TOKEN=%%a
echo Token set, starting OpenFang...
target\debug\openfang.exe start
pause
