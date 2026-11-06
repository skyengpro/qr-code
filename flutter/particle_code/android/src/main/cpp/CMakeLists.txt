cmake_minimum_required(VERSION 3.13)
project(particle_code_jni C)

add_library(particle_code_jni SHARED particle_code_jni.c)
# Android 15+ devices may use 16 KB pages.
target_link_options(particle_code_jni PRIVATE -Wl,-z,max-page-size=16384)
