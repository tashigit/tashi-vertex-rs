cmake_minimum_required(VERSION 4.2.3)
project(tashi_vertex_rs)

include(FetchContent)

set(TASHI_VERTEX_VERSION "0.13.0")
set(TASHI_VERTEX_URL "https://github.com/tashigg/tashi-vertex-c/releases/download/v${TASHI_VERTEX_VERSION}/tashi-vertex-${TASHI_VERTEX_VERSION}.zip")

# Local-build switch. Enable via `-DTASHI_VERTEX_LOCAL_BUILD=ON` or by exporting
# `TASHI_VERTEX_LOCAL_BUILD=1` before invoking cargo. When on, the build skips
if(DEFINED ENV{TASHI_VERTEX_LOCAL_BUILD} AND NOT "$ENV{TASHI_VERTEX_LOCAL_BUILD}" STREQUAL "")
    set(_tashi_vertex_local_default "$ENV{TASHI_VERTEX_LOCAL_BUILD}")
else()
    set(_tashi_vertex_local_default OFF)
endif()
option(TASHI_VERTEX_LOCAL_BUILD "Link against a locally-built tashi-vertex instead of downloading a release" ${_tashi_vertex_local_default})

set(_tashi_vertex_local_dir_default "${CMAKE_CURRENT_SOURCE_DIR}/../tashi-vertex-c")
if(DEFINED ENV{TASHI_VERTEX_LOCAL_DIR} AND NOT "$ENV{TASHI_VERTEX_LOCAL_DIR}" STREQUAL "")
    set(_tashi_vertex_local_dir_default "$ENV{TASHI_VERTEX_LOCAL_DIR}")
endif()
set(TASHI_VERTEX_LOCAL_DIR "${_tashi_vertex_local_dir_default}" CACHE PATH "Path to the local tashi-vertex-c checkout (containing lib/ and include/)")

if(WIN32)
    set(TASHI_VERTEX_ARCHIVE_LIB "tashi-vertex.dll")
    set(TASHI_VERTEX_INSTALLED_LIB "tashi-vertex.dll")
elseif(APPLE)
    set(TASHI_VERTEX_ARCHIVE_LIB "libtashi-vertex.dylib")
    set(TASHI_VERTEX_INSTALLED_LIB "libtashi-vertex.dylib")
elseif(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    if(CMAKE_SYSTEM_PROCESSOR MATCHES "^(aarch64|arm64)$")
        set(TASHI_VERTEX_ARCHIVE_LIB "libtashi-vertex-arm64.so")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "^riscv64")
        set(TASHI_VERTEX_ARCHIVE_LIB "libtashi-vertex-riscv64.so")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "^(x86_64|amd64|AMD64)$")
        set(TASHI_VERTEX_ARCHIVE_LIB "libtashi-vertex.so")
    else()
        message(FATAL_ERROR "Tashi Vertex: unsupported Linux architecture '${CMAKE_SYSTEM_PROCESSOR}'")
    endif()
    set(TASHI_VERTEX_INSTALLED_LIB "libtashi-vertex.so")
else()
    message(FATAL_ERROR "Tashi Vertex: unsupported platform '${CMAKE_SYSTEM_NAME}'")
endif()

if(TASHI_VERTEX_LOCAL_BUILD)
    set(TASHI_VERTEX_LIB_DIR "${TASHI_VERTEX_LOCAL_DIR}/lib")
    message(STATUS "Tashi Vertex: using local build from ${TASHI_VERTEX_LIB_DIR}")
    if(NOT EXISTS "${TASHI_VERTEX_LIB_DIR}/${TASHI_VERTEX_ARCHIVE_LIB}")
        message(FATAL_ERROR
            "Tashi Vertex: local library '${TASHI_VERTEX_ARCHIVE_LIB}' not found in "
            "'${TASHI_VERTEX_LIB_DIR}'. Run the tashi-vertex build script first, or "
            "set TASHI_VERTEX_LOCAL_DIR to the correct path.")
    endif()
else()
    message(STATUS "Tashi Vertex: fetching ${TASHI_VERTEX_URL}")
    FetchContent_Declare(
        TASHI_VERTEX
        URL ${TASHI_VERTEX_URL}
        DOWNLOAD_EXTRACT_TIMESTAMP TRUE
    )
    FetchContent_MakeAvailable(TASHI_VERTEX)
    set(TASHI_VERTEX_LIB_DIR "${tashi_vertex_SOURCE_DIR}/lib")
endif()

set(TASHI_VERTEX_LIB_FILE "${TASHI_VERTEX_LIB_DIR}/${TASHI_VERTEX_ARCHIVE_LIB}")

add_library(TASHI_VERTEX SHARED IMPORTED GLOBAL)
set_target_properties(TASHI_VERTEX PROPERTIES
    IMPORTED_LOCATION "${TASHI_VERTEX_LIB_FILE}"
)

install(FILES "${TASHI_VERTEX_LIB_FILE}"
    DESTINATION lib
    RENAME "${TASHI_VERTEX_INSTALLED_LIB}"
)
