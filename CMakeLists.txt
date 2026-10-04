cmake_minimum_required(VERSION 3.25)

project(opencascade_c_bridge VERSION 0.1.0 LANGUAGES C CXX)

include(CTest)
include(GNUInstallDirs)

find_package(
    OpenCASCADE REQUIRED
    COMPONENTS FoundationClasses ModelingData ModelingAlgorithms
)
set(
    OCCT_BRIDGE_OCCT_VERSION
    "${OpenCASCADE_MAJOR_VERSION}.${OpenCASCADE_MINOR_VERSION}.${OpenCASCADE_MAINTENANCE_VERSION}"
)
if(OCCT_BRIDGE_OCCT_VERSION VERSION_LESS 7.9.0)
    message(FATAL_ERROR "OCCT 7.9.0 or newer is required; found ${OCCT_BRIDGE_OCCT_VERSION}")
endif()

add_library(
    occt_bridge SHARED
    src/core.cpp
    src/session.cpp
    src/construction.cpp
    src/recipes.cpp
    src/solids.cpp
    src/operations.cpp
    src/inspection.cpp
    src/measurements.cpp
    src/surface_radius.cpp
    src/sweep.cpp
    src/step_assembly.cpp
    src/projection.cpp
    src/mesh.cpp
    src/history.cpp
    src/open_profile.cpp
    src/rib_support.cpp
    src/curvature.cpp
    src/exchange.cpp
    src/shape_validator.cpp
)
add_library(OpenCascadeCBridge::occt_bridge ALIAS occt_bridge)

target_compile_features(occt_bridge PRIVATE cxx_std_17)
target_compile_definitions(occt_bridge PRIVATE OCCT_BRIDGE_BUILD_SHARED)
target_compile_options(
    occt_bridge PRIVATE
    $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>
)
target_include_directories(
    occt_bridge
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
    PRIVATE
        ${OpenCASCADE_INCLUDE_DIR}
)
set(
    OCCT_BRIDGE_OCCT_LIBRARIES
    TKernel
    TKMath
    TKG2d
    TKG3d
    TKGeomBase
    TKBRep
    TKPrim
    TKTopAlgo
    TKBool
    TKFillet
    TKOffset
    TKXSBase
    TKDESTEP
    TKXCAF
    TKLCAF
    TKCAF
    TKCDF
    TKMesh
    TKHLR
    TKDESTL
    TKShHealing
)
target_link_libraries(occt_bridge PRIVATE ${OCCT_BRIDGE_OCCT_LIBRARIES})
set_target_properties(
    occt_bridge PROPERTIES
    CXX_VISIBILITY_PRESET hidden
    VISIBILITY_INLINES_HIDDEN YES
    VERSION ${PROJECT_VERSION}
    SOVERSION 1
)

if(UNIX AND NOT APPLE)
    target_link_options(
        occt_bridge PRIVATE
        "LINKER:--version-script,${CMAKE_CURRENT_SOURCE_DIR}/src/exports.map"
    )
endif()

if(BUILD_TESTING)
    add_executable(occt_bridge_c_test tests/c_api_test.c)
    target_compile_options(
        occt_bridge_c_test PRIVATE
        $<$<C_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>
    )
    target_link_libraries(occt_bridge_c_test PRIVATE OpenCascadeCBridge::occt_bridge m)
    add_test(NAME occt_bridge_c_api COMMAND occt_bridge_c_test)

    add_executable(occt_bridge_c_errors_test tests/c_api_errors_test.c)
    target_compile_options(
        occt_bridge_c_errors_test PRIVATE
        $<$<C_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>
    )
    target_link_libraries(occt_bridge_c_errors_test PRIVATE OpenCascadeCBridge::occt_bridge m)
    add_test(NAME occt_bridge_c_api_errors COMMAND occt_bridge_c_errors_test)

    # Internal validator, compiled in directly because the library hides it.
    add_executable(occt_bridge_shape_validator_test tests/shape_validator_test.cpp src/shape_validator.cpp)
    target_compile_features(occt_bridge_shape_validator_test PRIVATE cxx_std_17)
    target_compile_options(
        occt_bridge_shape_validator_test PRIVATE
        $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>
    )
    target_include_directories(occt_bridge_shape_validator_test PRIVATE ${OpenCASCADE_INCLUDE_DIR})
    target_link_libraries(occt_bridge_shape_validator_test PRIVATE ${OCCT_BRIDGE_OCCT_LIBRARIES})
    add_test(NAME occt_bridge_shape_validator COMMAND occt_bridge_shape_validator_test)

    # Force handle-space exhaustion to verify transactional multi-output APIs.
    add_executable(occt_bridge_handle_transaction_test tests/handle_transaction_test.cpp)
    target_compile_features(occt_bridge_handle_transaction_test PRIVATE cxx_std_17)
    target_compile_options(occt_bridge_handle_transaction_test PRIVATE $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>)
    target_include_directories(occt_bridge_handle_transaction_test PRIVATE src ${OpenCASCADE_INCLUDE_DIR})
    target_link_libraries(occt_bridge_handle_transaction_test PRIVATE OpenCascadeCBridge::occt_bridge ${OCCT_BRIDGE_OCCT_LIBRARIES})
    add_test(NAME occt_bridge_handle_transactions COMMAND occt_bridge_handle_transaction_test)

    # Read STEP assemblies back through XCAF to verify structure, names, and colors.
    add_executable(occt_bridge_step_assembly_test tests/step_assembly_test.cpp)
    target_compile_features(occt_bridge_step_assembly_test PRIVATE cxx_std_17)
    target_compile_options(occt_bridge_step_assembly_test PRIVATE $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>)
    target_include_directories(occt_bridge_step_assembly_test PRIVATE ${OpenCASCADE_INCLUDE_DIR})
    target_link_libraries(occt_bridge_step_assembly_test PRIVATE OpenCascadeCBridge::occt_bridge ${OCCT_BRIDGE_OCCT_LIBRARIES})
    add_test(NAME occt_bridge_step_assembly
        COMMAND occt_bridge_step_assembly_test ${CMAKE_CURRENT_BINARY_DIR}/step-assembly-test.step)
endif()

install(
    TARGETS occt_bridge
    EXPORT OpenCascadeCBridgeTargets
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)
install(FILES include/occt_bridge.h DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
