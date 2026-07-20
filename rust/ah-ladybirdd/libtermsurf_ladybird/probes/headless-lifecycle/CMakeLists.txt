cmake_minimum_required(VERSION 3.30)

project(TermSurfLadybirdHeadlessProbe LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT DEFINED LADYBIRD_SOURCE_DIR)
  message(FATAL_ERROR "LADYBIRD_SOURCE_DIR is required")
endif()

if(NOT DEFINED LADYBIRD_BUILD_DIR)
  message(FATAL_ERROR "LADYBIRD_BUILD_DIR is required")
endif()

add_executable(ladybird_headless_lifecycle_probe headless_lifecycle_probe.cpp)

target_include_directories(ladybird_headless_lifecycle_probe PRIVATE
  "${LADYBIRD_SOURCE_DIR}"
  "${LADYBIRD_SOURCE_DIR}/AK"
  "${LADYBIRD_SOURCE_DIR}/Libraries"
  "${LADYBIRD_BUILD_DIR}"
  "${LADYBIRD_BUILD_DIR}/Libraries"
)

target_link_directories(ladybird_headless_lifecycle_probe PRIVATE
  "${LADYBIRD_BUILD_DIR}/lib"
  "${LADYBIRD_BUILD_DIR}/Libraries"
)

target_link_libraries(ladybird_headless_lifecycle_probe PRIVATE
  LibWebView
  LibWeb
  LibCore
  LibGfx
  LibURL
  AK
)
