# SPDX-License-Identifier: GPL-3.0-only
set(version_file "${CMAKE_CURRENT_LIST_DIR}/../VERSION")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${version_file}")
file(STRINGS "${version_file}" SPOOLCUT_VERSION LIMIT_COUNT 1)
unset(version_file)
if(NOT SPOOLCUT_VERSION MATCHES "^([0-9]+\\.[0-9]+\\.[0-9]+)(-[0-9A-Za-z]+(\\.[0-9A-Za-z]+)*)?$")
    message(FATAL_ERROR "Invalid SpoolCut version in VERSION: ${SPOOLCUT_VERSION}")
endif()
set(SPOOLCUT_NUMERIC_VERSION "${CMAKE_MATCH_1}")

find_package(Git QUIET)
set(SPOOLCUT_BUILD_REVISION "")
if(Git_FOUND AND EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/.git")
    execute_process(COMMAND "${GIT_EXECUTABLE}" rev-parse --verify HEAD
        WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
        OUTPUT_VARIABLE revision OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
    if(revision MATCHES "^[0-9a-f]+$")
        set(SPOOLCUT_BUILD_REVISION "${revision}")
    endif()
endif()
