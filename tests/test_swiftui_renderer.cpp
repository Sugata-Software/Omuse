#include <QContextMenuEvent>
#include <QLabel>
#include <QMenu>
#include <QMouseEvent>
#include <QTimer>
// Qt adapter regression tests. ABI stubs isolate notification lifetime from Swift
// while the host --ui-smoke covers the complete editor and rendered pixels.
#include "SwiftUIQtRenderer.h"
#include <QApplication>
#include <QCoreApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QPointer>
#include <QPushButton>
#include <QLineEdit>
#include <cstring>
#include <cstdio>
#include <memory>

static uint64_t lastSession = 0;
static QString lastHandler;
static QJsonObject extraTree;
static int dispatchResult = 0;
static double value = 12;
extern "C" int64_t compositor_session_render_tree(uint64_t, const char *, uint8_t *output, size_t capacity) {
    const QJsonObject button{{"id", "0.0"}, {"kind", "Button"}, {"handlerKeys", QJsonArray{"action"}},
        {"modifiers", QJsonArray{QJsonObject{{"kind", "accessibilityLabel"}, {"stringParams", QJsonObject{{"text", "Run action"}}}}}}};
    const QJsonObject field{{"id", "0.1"}, {"kind", "TextField"}, {"handlerKeys", QJsonArray{"value"}},
        {"doubleParams", QJsonObject{{"value", value}}}};
    const QByteArray bytes = QJsonDocument(extraTree.isEmpty() ? QJsonObject{{"id", "0"}, {"kind", "HStack"}, {"children", QJsonArray{button, field}}} : extraTree).toJson();
    if (output && capacity >= size_t(bytes.size())) std::memcpy(output, bytes.data(), bytes.size());
    return bytes.size();
}
extern "C" int32_t compositor_session_dispatch_swiftui_action(uint64_t handle, const char *, const char *, const char *handler, const uint8_t *, size_t) { lastSession=handle; lastHandler=QString::fromUtf8(handler); return dispatchResult; }
extern "C" int64_t compositor_session_render_swiftui_canvas(uint64_t, const char *, const char *, size_t, size_t, uint8_t *, size_t) { return -1; }
#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "FAIL at line %d: %s\n", __LINE__, #condition); return 1; } } while (0)

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    auto pump = [] { QCoreApplication::processEvents(); QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete); };
    std::unique_ptr<QWidget> panel(swiftUIRenderPanel(1, "test"));
    CHECK(panel);
    auto *button = panel->findChild<QPushButton *>();
    auto *field = panel->findChild<QLineEdit *>();
    CHECK(button && field && button->accessibleName() == "Run action");
    int calls = 0;
    auto owner = std::make_unique<QObject>();
    registerSwiftUIActionListener(owner.get(), [&](uint64_t, const QString &) { ++calls; });
    button->click();
    CHECK(calls == 0); // notifications cannot reenter the active Qt signal
    pump(); CHECK(calls == 1);
    dispatchResult = -1;
    button->click(); pump(); CHECK(calls == 1); // failed dispatch is not a model change
    dispatchResult = 0;
    button->click(); owner.reset(); pump(); CHECK(calls == 1); // queued receiver removed
    button->click(); pump(); CHECK(calls == 1); // registry entry removed too
    QPointer<QLineEdit> original(field);
    value = 42;
    CHECK(swiftUIRenderPanel(1, "test", panel.get()) == panel.get());
    CHECK(original && original->text() == "42");
    panel->show(); panel->activateWindow(); field->setFocus(); pump();
    field->setText("4."); field->setCursorPosition(2);
    value = 4;
    CHECK(swiftUIRenderPanel(1, "test", panel.get()) == panel.get());
    CHECK(original && original->hasFocus() && original->text() == "4." && original->cursorPosition() == 2);
    std::unique_ptr<QWidget> replacement(swiftUIRenderPanel(2, "test", panel.get()));
    CHECK(replacement && replacement.get() != panel.get());
    replacement->findChild<QPushButton *>()->click();
    CHECK(lastSession == 2); // Reopening a project changes the live editor handle.
    std::unique_ptr<QWidget> otherPanel(swiftUIRenderPanel(2, "other", replacement.get()));
    CHECK(otherPanel && otherPanel.get() != replacement.get());
    extraTree=QJsonDocument::fromJson(R"({"id":"0","kind":"HStack","handlerKeys":["submit"],"children":[
      {"id":"0.0","kind":"Text","stringParams":{"text":"Tap"},"handlerKeys":["tap","doubleTap"],"children":[
        {"id":"0.0.0","kind":"ContextMenu","children":[{"id":"0.0.0.0","kind":"Button","handlerKeys":["action"],"children":[{"kind":"Text","stringParams":{"text":"Do it"}}]}]}]},
      {"id":"0.1","kind":"TextField","handlerKeys":["text"],"stringParams":{"text":"value"}}
    ]})").object();
    std::unique_ptr<QWidget> interactions(swiftUIRenderPanel(1,"test"));
    interactions->show(); pump();
    auto *tap=interactions->findChild<QLabel *>(); CHECK(tap);
    QMouseEvent released(QEvent::MouseButtonRelease,QPointF(2,2),QPointF(2,2),Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
    QApplication::sendEvent(tap,&released); CHECK(lastHandler=="tap");
    QMouseEvent twice(QEvent::MouseButtonDblClick,QPointF(2,2),QPointF(2,2),Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
    QApplication::sendEvent(tap,&twice); CHECK(lastHandler=="doubleTap");
    auto *edit=interactions->findChild<QLineEdit *>(); CHECK(edit);
    QMetaObject::invokeMethod(edit,"returnPressed",Qt::DirectConnection); CHECK(lastHandler=="submit");
    bool menuVisited=false;
    QTimer::singleShot(0,[&] {auto *menu=qobject_cast<QMenu *>(QApplication::activePopupWidget());if(menu && !menu->actions().isEmpty()){menuVisited=true;menu->actions().first()->trigger();menu->close();}});
    QContextMenuEvent context(QContextMenuEvent::Mouse,QPoint(2,2),tap->mapToGlobal(QPoint(2,2)));
    QApplication::sendEvent(tap,&context);
    CHECK(menuVisited && lastHandler=="action");
    puts("Qt renderer lifetime, failure, accessibility and reconciliation checks passed");
}
