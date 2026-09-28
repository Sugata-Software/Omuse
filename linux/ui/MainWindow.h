#ifndef MainWindow_h
#define MainWindow_h

#include <QMainWindow>

// Plan §7: MainWindow is the C++-only Qt shell (ENG-2 composition root).
// It hosts a SessionWindow (moc'd Q_OBJECT) which drives the Swift core
// via the compositor_session_* C ABI. The richer UI controls (layer tree,
// toolbars) live here; the Swift-side model (EditorSession) remains the
// single source of truth for document state.
// SessionWindow is forward-declared; the full include is in MainWindow.cpp.

class SessionWindow;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    MainWindow(QWidget *parent = nullptr);
    ~MainWindow() override;

    void set_session_window(SessionWindow *window);

private:
    SessionWindow *m_session_window;
};

#endif /* MainWindow_h */