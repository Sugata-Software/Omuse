#include "MainWindow.h"
#include <QMenuBar>
#include <QMenu>
#include <QAction>
#include <QWidget>

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent)
{
    setWindowTitle("Compositor");
    resize(800, 600);

    canvas = new QWidget(this);
    setCentralWidget(canvas);

    QMenu *fileMenu = menuBar()->addMenu("&File");
    QAction *quitAction = fileMenu->addAction("&Quit");
    connect(quitAction, &QAction::triggered, this, &QMainWindow::close);
}

MainWindow::~MainWindow()
{
}
