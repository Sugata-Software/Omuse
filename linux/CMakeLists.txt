cmake_minimum_required(VERSION 3.16)
project(Compositor VERSION 1.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 REQUIRED COMPONENTS Widgets)

add_executable(compositor
    main.cpp
    MainWindow.cpp
    MainWindow.h
)

target_link_libraries(compositor PRIVATE Qt6::Widgets)
