enable_testing()

find_package(Threads REQUIRED)
find_package(OpenSSL REQUIRED Crypto)

add_library(muabase_db
   src/sal.cpp
   src/cow_art.cpp
   src/dwal_merkle.cpp
   src/tx_session.cpp)
target_include_directories(muabase_db PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(muabase_db PUBLIC Threads::Threads OpenSSL::Crypto)

add_executable(muabase-db-sal-tests test/sal_bdd_tests.cpp)
target_link_libraries(muabase-db-sal-tests PUBLIC muabase_db)
set_target_properties(muabase-db-sal-tests PROPERTIES RUNTIME_OUTPUT_DIRECTORY ${ROOT_BINARY_DIR})
native_test(muabase-db-sal-tests)

add_executable(muabase-db-art-tests test/cow_art_bdd_tests.cpp)
target_link_libraries(muabase-db-art-tests PUBLIC muabase_db)
set_target_properties(muabase-db-art-tests PROPERTIES RUNTIME_OUTPUT_DIRECTORY ${ROOT_BINARY_DIR})
native_test(muabase-db-art-tests)

add_executable(muabase-db-dwal-tests test/dwal_merkle_bdd_tests.cpp)
target_link_libraries(muabase-db-dwal-tests PUBLIC muabase_db)
set_target_properties(muabase-db-dwal-tests PROPERTIES RUNTIME_OUTPUT_DIRECTORY ${ROOT_BINARY_DIR})
native_test(muabase-db-dwal-tests)

add_executable(muabase-db-tx-tests test/tx_session_bdd_tests.cpp)
target_link_libraries(muabase-db-tx-tests PUBLIC muabase_db)
set_target_properties(muabase-db-tx-tests PROPERTIES RUNTIME_OUTPUT_DIRECTORY ${ROOT_BINARY_DIR})
native_test(muabase-db-tx-tests)
