#include "MainWindow.h"
#include <QVBoxLayout>
#include <QDebug>

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent)
{
    // Create the SessionWindow which is the moc'd Q_OBJECT that drives
    // the Swift core via composer_session_* and paints the composited RGBA.
    m_session_window = new SessionWindow(this);
    setCentralWidget(m_session_window);
    resize(800, 600);
    setWindowTitle("Compositor — Linux Port");
}

MainWindow::~MainWindow() {
    // SessionWindow is owned by this MainWindow via parenthood; deleted automatically.
}

void MainWindow::set_session_window(SessionWindow *window) {
    // If a different window is provided, reparent.
    if (m_session_window != window) {
        if (m_session_window) {
            m_session_window->setParent(nullptr);
            m_session_window->deleteLater();
        }
        m_session_window = window;
        if (m_session_window) {
            m_session_window->setParent(this);
            setCentralWidget(m_session_window);
        }
    }
}