# SPDX-License-Identifier: GPL-3.0-only
cmake_minimum_required(VERSION 3.25)
include(cmake/SpoolCutVersion.cmake)
project(SpoolCut VERSION "${SPOOLCUT_VERSION}" LANGUAGES CXX)

include(CTest)
find_package(Qt6 6.8 REQUIRED COMPONENTS Core)
message(STATUS "Using Qt ${Qt6_VERSION}")

add_subdirectory(cpp)

if(BUILD_TESTING)
    find_package(Python3 REQUIRED COMPONENTS Interpreter)
    set(SPOOLCUT_RUST_REFERENCE "" CACHE FILEPATH "Rust CLI built at 39cb4ee for differential tests")

    add_executable(spoolcut-core-tests cpp/tests/core.cpp)
    target_link_libraries(spoolcut-core-tests PRIVATE spoolcut-core)
    spoolcut_target(spoolcut-core-tests)
    add_test(NAME core-unit COMMAND spoolcut-core-tests)

    foreach(case avc hevc main10 vfr short-tail rejected interrupt)
        set(reference_args)
        if(SPOOLCUT_RUST_REFERENCE)
            set(reference_args --reference "${SPOOLCUT_RUST_REFERENCE}")
        endif()
        add_test(NAME stream-copy-${case}
            COMMAND Python3::Interpreter "${CMAKE_CURRENT_SOURCE_DIR}/cpp/tests/differential.py"
                "${case}" --cli $<TARGET_FILE:spoolcut> --core-tests $<TARGET_FILE:spoolcut-core-tests>
                ${reference_args})
        set_tests_properties(stream-copy-${case} PROPERTIES TIMEOUT 120)
    endforeach()
endif()
