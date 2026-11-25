# SPDX-License-Identifier: GPL-3.0-only
set(version_file "${CMAKE_CURRENT_LIST_DIR}/../VERSION")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${version_file}")
file(STRINGS "${version_file}" SPOOLCUT_VERSION LIMIT_COUNT 1)
unset(version_file)
