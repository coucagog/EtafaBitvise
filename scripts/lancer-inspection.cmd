@echo off
rem Lance MLS Connect (build de debogage) avec l'inspection WebView2 sur 127.0.0.1:9333, pour diagnostic.
call "%~dp0lancer.cmd" "--remote-debugging-port=9333"
