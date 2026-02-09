cmake_minimum_required(VERSION 3.23)
project(tashi_vertex_rs)

include(FetchContent)

# Set the GitHub archive URL and version
set(TASHI_VERTEX_VERSION "0.12.0")
set(TASHI_VERTEX_URL "https://github.com/tashigg/tashi-vertex-c/releases/download/v${TASHI_VERTEX_VERSION}/tashi-vertex-${TASHI_VERTEX_VERSION}.zip")

# Download and extract the pre-built library
FetchContent_Declare(
    TASHI_VERTEX
    URL ${TASHI_VERTEX_URL}
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE
)

FetchContent_MakeAvailable(TASHI_VERTEX)

# Set library paths based on the extracted content
set(TASHI_VERTEX_LIB_DIR "${tashi_vertex_SOURCE_DIR}/lib")

# Create an imported library target
add_library(TASHI_VERTEX SHARED IMPORTED GLOBAL)

# Set the library file location (adjust extension based on platform)
if(WIN32)
    set(TASHI_VERTEX_LIB_FILE "${TASHI_VERTEX_LIB_DIR}/tashi-vertex.dll")
    set_target_properties(TASHI_VERTEX PROPERTIES
        IMPORTED_LOCATION "${TASHI_VERTEX_LIB_FILE}"
    )
elseif(APPLE)
    set(TASHI_VERTEX_LIB_FILE "${TASHI_VERTEX_LIB_DIR}/libtashi-vertex.dylib")
    set_target_properties(TASHI_VERTEX PROPERTIES
        IMPORTED_LOCATION "${TASHI_VERTEX_LIB_FILE}"
    )
else()
    set(TASHI_VERTEX_LIB_FILE "${TASHI_VERTEX_LIB_DIR}/libtashi-vertex.so")
    set_target_properties(TASHI_VERTEX PROPERTIES
        IMPORTED_LOCATION "${TASHI_VERTEX_LIB_FILE}"
    )
endif()

# Install the library
if(WIN32)
    install(FILES ${TASHI_VERTEX_LIB_FILE} "${TASHI_VERTEX_LIB_DIR}/tashi-vertex.lib"
        DESTINATION lib
    )
else()
    install(FILES ${TASHI_VERTEX_LIB_FILE}
        DESTINATION lib
    )
endif()
