# SPDX-License-Identifier: GPL-3.0-only
cmake_minimum_required(VERSION 3.25)
project(SpoolCut VERSION 0.1.0 LANGUAGES CXX)

include(CTest)
find_package(Qt6 6.8 REQUIRED COMPONENTS Core)

option(SPOOLCUT_SANITIZERS "Build with AddressSanitizer and UndefinedBehaviorSanitizer" OFF)

function(spoolcut_target target)
    target_compile_features(${target} PUBLIC cxx_std_20)
    set_target_properties(${target} PROPERTIES CXX_EXTENSIONS OFF)
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Wconversion -Wsign-conversion)
    if(SPOOLCUT_SANITIZERS)
        target_compile_options(${target} PRIVATE -fsanitize=address,undefined -fno-omit-frame-pointer)
        target_link_options(${target} PRIVATE -fsanitize=address,undefined)
    endif()
endfunction()

add_library(spoolcut-core STATIC
    cpp/core/project.cpp
    cpp/core/platform.cpp
    cpp/core/media.cpp
    cpp/core/plan.cpp
    cpp/core/export.cpp)
target_include_directories(spoolcut-core PUBLIC cpp/core)
target_link_libraries(spoolcut-core PUBLIC Qt6::Core)
spoolcut_target(spoolcut-core)

add_executable(spoolcut cpp/main.cpp)
target_link_libraries(spoolcut PRIVATE spoolcut-core)
spoolcut_target(spoolcut)

if(BUILD_TESTING)
    find_package(Python3 REQUIRED COMPONENTS Interpreter)
    set(SPOOLCUT_RUST_REFERENCE "" CACHE FILEPATH "Rust CLI built at 39cb4ee for differential tests")

    add_executable(spoolcut-core-tests cpp/tests/core.cpp)
    target_link_libraries(spoolcut-core-tests PRIVATE spoolcut-core)
    spoolcut_target(spoolcut-core-tests)
    add_test(NAME core-unit COMMAND spoolcut-core-tests)

    foreach(case avc hevc main10 vfr rejected interrupt)
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
    set_tests_properties(stream-copy-interrupt PROPERTIES SKIP_RETURN_CODE 77)
endif()
