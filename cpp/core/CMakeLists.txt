# SPDX-License-Identifier: GPL-3.0-only
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
    project.cpp
    platform.cpp
    media.cpp
    plan.cpp
    export.cpp)
target_include_directories(spoolcut-core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(spoolcut-core PUBLIC Qt6::Core)
spoolcut_target(spoolcut-core)

