enable_testing()

find_package(Threads REQUIRED)
find_package(OpenSSL REQUIRED Crypto)

add_library(muabase_db
   src/sal.cpp)
target_include_directories(muabase_db PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(muabase_db PUBLIC Threads::Threads OpenSSL::Crypto)

add_executable(muabase-db-sal-tests test/sal_bdd_tests.cpp)
target_link_libraries(muabase-db-sal-tests PUBLIC muabase_db)
set_target_properties(muabase-db-sal-tests PROPERTIES RUNTIME_OUTPUT_DIRECTORY ${ROOT_BINARY_DIR})
native_test(muabase-db-sal-tests)
