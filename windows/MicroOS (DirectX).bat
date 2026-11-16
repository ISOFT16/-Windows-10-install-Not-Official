@echo off
rem Start MicroOS via DirectX (ANGLE) if MicroOS.exe fails because of old video drivers.
set KIVY_GL_BACKEND=angle_sdl2
start "" "%~dp0MicroOS.exe"
