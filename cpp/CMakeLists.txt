# SPDX-License-Identifier: GPL-3.0-only
add_subdirectory(core)

add_executable(spoolcut main.cpp)
target_link_libraries(spoolcut PRIVATE spoolcut-core)
spoolcut_target(spoolcut)
target_compile_definitions(spoolcut PRIVATE SPOOLCUT_VERSION="${SPOOLCUT_VERSION}")
set_target_properties(spoolcut PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
