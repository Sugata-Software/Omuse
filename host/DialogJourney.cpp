#include <QFileDialog>
#include <QScrollArea>
#include <QScrollBar>
#include <QMessageBox>
#include <QTabBar>
#include <QContextMenuEvent>
#include <QFile>
#include <QProcess>
#include <QProcessEnvironment>
#include <QTextEdit>
#include "LayerDragModel.h"
#include <QKeySequenceEdit>
#include <QDragEnterEvent>
#include <QDragMoveEvent>
#include <QDropEvent>
#include <QMenu>
// Qt event-loop journey, invoked by the Swift composition root with --dialog-smoke.
// Uses actual menu actions, modal controls, codecs, and the real Swift session.
#include "SessionWindow.h"
#include "EditorDialogs.h"
#include "compositor_host_run.h"
#include "interfaces/IPlatformServices.h"
#include <QAction>
#include <QPointer>
#include <QKeyEvent>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QImageReader>
#include <QLabel>
#include <QJsonArray>
#include <QJsonDocument>
#include <QTreeView>
#include <QSpinBox>
#include <QMouseEvent>
#include <QStandardItemModel>
#include <QPushButton>
#include <QSlider>
#include <QLineEdit>
#include <QListWidget>
#include <QTemporaryDir>
#include <QFileInfo>
#include <QDir>
#include <QTimer>
#include <QColor>
#include <QDebug>
#include <stdexcept>

namespace {
void require(bool ok, const char *message) { if (!ok) throw std::runtime_error(message); }
QImage exported(SessionWindow &window, const QString &path) {
    require(window.exportPNG(path), "PNG export failed");
    QImage image(path);
    require(!image.isNull(), "PNG read failed");
    return image;
}
void modal(SessionWindow &window, const QString &actionName, const std::function<void(QDialog *)> &body) {
    auto *action = window.findChild<QAction *>(actionName);
    require(action != nullptr, "menu action missing");
    QString error;
    bool visited = false;
    QTimer dispatch;
    dispatch.setSingleShot(true);
    QObject::connect(&dispatch, &QTimer::timeout, &window, [&] {
        visited = true;
        auto *dialog = qobject_cast<QDialog *>(QApplication::activeModalWidget());
        if (!dialog) { error = "dialog missing"; return; }
        try { body(dialog); }
        catch (const std::exception &e) { error = e.what(); dialog->reject(); }
    });
    dispatch.start(0);
    action->trigger();
    require(visited, "action did not open a dialog");
    require(error.isEmpty(), qPrintable(error));
}
void click(QDialog *dialog, QDialogButtonBox::StandardButton which) {
    auto *buttons = dialog->findChild<QDialogButtonBox *>();
    require(buttons && buttons->button(which)->isEnabled(), "dialog action disabled");
    buttons->button(which)->click();
}
QDoubleSpinBox *number(QDialog *dialog, const char *name) {
    auto *field = dialog->findChild<QDoubleSpinBox *>(name);
    require(field != nullptr, "numeric control missing"); return field;
}
void undo(SessionWindow &window) {
    for (auto *action : window.findChildren<QAction *>()) {
        if (action->shortcut() == QKeySequence(QKeySequence::Undo)) { action->trigger(); return; }
    }
    throw std::runtime_error("undo action missing");
}
}


// Test doubles for the platform seams (Dependency Inversion): the shell must run
// against these with no Qt file dialog, message box, clipboard or app-data lookup.
namespace {
struct FakeFiles final : IFileDialogService {
    QString exportPath, openImagePath, projectPath, savePath;
    int saveRequests = 0;
    QString chooseImageToOpen() override { return openImagePath; }
    QString chooseProjectToOpen() override { return projectPath; }
    QString chooseProjectSavePath() override { ++saveRequests; return savePath; }
    QString chooseExportPath(const QString &, const QString &) override { return exportPath; }
};
struct FakeClipboard final : IClipboardService {
    QImage stored;
    void setImage(const QImage &image) override { stored = image; }
    QImage image() const override { return stored; }
};
struct FakeNotifier final : IUserNotifier {
    QStringList warnings;
    void warn(const QString &title, const QString &) override { warnings << title; }
};
struct FakeStorage final : IStorageLocator {
    QString root;
    QString appDataDirectory() const override { return root; }
};
}

namespace {
void answerMessage(QWidget &owner, QMessageBox::StandardButton answer) {
    QTimer::singleShot(0, &owner, [answer] {
        auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget());
        if (box && box->button(answer)) box->button(answer)->click();
        else if (auto *dialog = qobject_cast<QDialog *>(QApplication::activeModalWidget())) dialog->reject();
    });
}
void documentSafetyJourney() {
    QTemporaryDir temporary;
    auto files = std::make_shared<FakeFiles>();
    auto notifier = std::make_shared<FakeNotifier>();
    auto storage = std::make_shared<FakeStorage>(); storage->root = temporary.filePath("data");
    PlatformServices services; services.files = files; services.notifier = notifier; services.storage = storage;
    SessionWindow window(nullptr, services); window.show(); QApplication::processEvents();
    const QString png = temporary.filePath("view.png");
    files->savePath = temporary.filePath("My artwork");
    window.findChild<QAction *>("file.save")->trigger();
    const QString project = files->savePath + ".comp";
    require(QFileInfo::exists(project + "/manifest.json"), "first Save did not create a project with extension");
    require(!window.hasUnsavedChanges(), "successful save left document dirty");
    require(window.windowTitle().contains("My artwork.comp"), "saved document title missing");
    window.paintStroke(3, 3, 18, 12);
    require(window.hasUnsavedChanges(), "painting not marked dirty");
    window.findChild<QAction *>("file.save")->trigger();
    require(files->saveRequests == 1, "Save asked for path again");
    const QImage saved = exported(window, png);
    undo(window); require(window.hasUnsavedChanges(), "undo away from saved revision not dirty");
    window.findChild<QAction *>("edit.redo")->trigger();
    require(!window.hasUnsavedChanges(), "redo to saved revision not clean");
    window.paintStroke(40, 40, 50, 50);
    require(window.performAutosave(), "dirty autosave failed");
    const QImage dirty = exported(window, png);
    answerMessage(window, QMessageBox::Cancel);
    require(!window.close() && window.isVisible(), "Cancel closed edited document");
    require(window.hasAutosaveRecovery() && exported(window, png) == dirty, "cancel destroyed edits or recovery");
    files->savePath.clear();
    window.findChild<QAction *>("file.saveAs")->trigger();
    require(window.hasUnsavedChanges(), "cancelled Save As cleared dirty state");
    files->savePath = temporary.filePath("missing/blocked.comp");
    // A regular file in the parent path makes the failure deterministic, even as root.
    QFile blocked(temporary.filePath("missing")); require(blocked.open(QIODevice::WriteOnly), "fixture failed"); blocked.close();
    window.findChild<QAction *>("file.saveAs")->trigger();
    require(!notifier->warnings.isEmpty() && window.hasUnsavedChanges(), "failed save was not reported or cleared dirty state");
    require(window.hasAutosaveRecovery(), "failed save removed recovery");
    // Failed Open after Discard must retain the live document and its recovery.
    files->projectPath = temporary.filePath("broken.comp"); QDir().mkpath(files->projectPath);
    answerMessage(window, QMessageBox::Discard);
    window.findChild<QAction *>("file.openProject")->trigger();
    require(exported(window, png) == dirty && window.hasAutosaveRecovery(), "failed Open destroyed current work");
    files->projectPath = project;
    answerMessage(window, QMessageBox::Cancel);
    window.findChild<QAction *>("file.openProject")->trigger();
    require(exported(window, png) == dirty, "cancelled Open replaced work");
    answerMessage(window, QMessageBox::Discard);
    window.findChild<QAction *>("file.openProject")->trigger();
    require(exported(window, png) == saved && !window.hasUnsavedChanges(), "Open did not restore saved pixels cleanly");
    window.setTool(SessionWindow::Tool::Brush); QApplication::processEvents();
    auto *brushHeader = window.findChild<QWidget *>("swiftUIOptionsContainer");
    QLineEdit *brushSize=nullptr;
    for (auto *field : brushHeader->findChildren<QLineEdit *>()) if (field->isVisible()) { brushSize=field; break; }
    require(brushSize, "reopened brush control missing");
    brushSize->setText("27"); QApplication::processEvents();
    require(window.sessionState().value("brush").toObject().value("diameter").toInt()==27,
            "reopened controls still target the closed editor session");
    modal(window, "file.new", [&](QDialog *dialog) {
        require(dialog->objectName() == "newCanvas.dialog", "New opened resize dialog");
        dialog->findChild<QSpinBox *>("width")->setValue(80);
        dialog->findChild<QSpinBox *>("height")->setValue(48);
        click(dialog, QDialogButtonBox::Ok);
    });
    const QImage blank = exported(window, png);
    require(blank.size() == QSize(80,48), "New canvas dimensions wrong");
    for (int y=0; y<blank.height(); ++y) for (int x=0; x<blank.width(); ++x)
        require(blank.pixelColor(x,y).alpha() == 0, "New canvas retained old artwork");
    require(window.windowTitle().contains("Untitled"), "New kept old save destination");
    QImage source(17,23,QImage::Format_RGBA8888); source.fill(Qt::green);
    files->openImagePath=temporary.filePath("source.png"); require(source.save(files->openImagePath),"image fixture failed");
    window.findChild<QAction *>("file.importImages")->trigger();
    require(window.sessionState().value("width").toInt()==80 && window.sessionState().value("layers").toArray().size()==2,
            "Import replaced canvas instead of adding layer");
    require(exported(window,png)!=blank, "Import did not refresh pixels");
    require(window.performAutosave(), "import did not become recoverable");
    const QString ownRecovery=window.autosaveDirectory();
    QString peerRecovery;
    {
        SessionWindow peer(nullptr,services); peerRecovery=peer.autosaveDirectory();
        require(peerRecovery!=ownRecovery,"windows share recovery directory");
        require(peer.performAutosave(),"second window recovery failed");
        SessionWindow probe(nullptr,services); probe.offerRecovery(); // Must skip both live lock owners without prompting.
        window.clearAutosave();
        require(peer.hasAutosaveRecovery(),"one window cleared another window's recovery");
        // Destruction without close simulates an interrupted process retaining recovery.
    }
    SessionWindow recovered(nullptr,services); recovered.show();
    answerMessage(recovered,QMessageBox::Yes); recovered.offerRecovery();
    require(recovered.hasUnsavedChanges(),"recovered project treated as saved");
    require(QFileInfo::exists(peerRecovery+"/autosave.comp/manifest.json"),"recovery source removed before user saved");
    files->savePath.clear();
    answerMessage(recovered,QMessageBox::Save);
    require(!recovered.close(),"cancelled save during close discarded recovered document");
    files->savePath=temporary.filePath("missing/blocked.comp");
    answerMessage(recovered,QMessageBox::Save);
    require(!recovered.close() && recovered.isVisible(),"failed save during close discarded work");
    files->savePath=temporary.filePath("Recovered.comp");
    answerMessage(recovered,QMessageBox::Save);
    require(recovered.close(),"Save and close failed");
    require(QFileInfo::exists(files->savePath+"/manifest.json"),"recovery Save did not persist");
    require(!QFileInfo::exists(peerRecovery+"/autosave.comp"),"saved recovery source not retired");
    require(window.performAutosave(),"autosave after another window closed failed");
    answerMessage(window,QMessageBox::Discard);
    auto *tabs=window.findChild<QTabBar *>("header.documentTabs");
    auto *tabClose=qobject_cast<QAbstractButton *>(tabs->tabButton(0,QTabBar::RightSide));
    require(tabClose && tabClose->isVisible(),"document tab close button missing");
    tabClose->click();
    require(!window.isVisible() && !window.hasAutosaveRecovery(),"tab close did not honor explicit Discard");
    qInfo("Document safety journey OK (Save/Save As, dirty history, close/cancel/failure, New, Open, import, independent recovery, startup recovery)");
}
}

extern "C" int compositor_host_dialog_smoke(int argc, char **argv) {
    QApplication app(argc, argv);
    try {
        QTemporaryDir temporary;
        require(temporary.isValid(), "temporary directory failed");
        SessionWindow window; window.show(); QApplication::processEvents();
        const QString path = temporary.filePath("view.png");
        const QImage original = exported(window, path);
        modal(window, "canvasSize", [&](QDialog *dialog) {
            number(dialog, "width")->setValue(96);
            click(dialog, QDialogButtonBox::Cancel);
        });
        require(exported(window, path) == original, "cancel resized canvas");
        modal(window, "canvasSize", [&](QDialog *dialog) {
            number(dialog, "width")->setValue(96);
            dialog->findChild<QComboBox *>("anchor")->setCurrentIndex(0);
            dialog->findChild<QComboBox *>("extension")->setCurrentIndex(2);
            click(dialog, QDialogButtonBox::Ok);
        });
        const QImage canvas = exported(window, path);
        require(canvas.size() == QSize(96, 64), "canvas refresh kept stale dimensions");
        require(canvas.pixelColor(95, 63) == QColor(Qt::white), "canvas extension missing");
        undo(window); require(exported(window, path) == original, "canvas undo failed");
        modal(window, "imageSize", [&](QDialog *dialog) {
            dialog->findChild<QCheckBox *>("resample")->setChecked(false);
            number(dialog, "resolution")->setValue(300);
            click(dialog, QDialogButtonBox::Ok);
        });
        const QImage print = exported(window, path);
        require(print.size() == original.size(), "resolution-only resampled dimensions");
        require(qAbs(print.dotsPerMeterX() - qRound(300 / 0.0254)) <= 1, "export resolution lost");
        undo(window);
        modal(window, "imageSize", [&](QDialog *dialog) {
            dialog->findChild<QComboBox *>("units")->setCurrentIndex(1);
            number(dialog, "width")->setValue(50);
            click(dialog, QDialogButtonBox::Ok);
        });
        require(exported(window, path).size() == QSize(32, 32), "image resize/ratio failed");
        undo(window); require(exported(window, path) == original, "image undo failed");

        // Let the queued preview run, then exercise visibility and Escape rollback.
        QString previewError;
        modal(window, "filter.Gaussian Blur", [&](QDialog *dialog) {
            QTimer::singleShot(200, dialog, [&, dialog] {
                try {
                    require(exported(window, path) != original, "filter preview did not render");
                    auto *preview = dialog->findChild<QCheckBox *>("preview");
                    require(preview != nullptr, "preview checkbox missing");
                    preview->setChecked(false);
                    require(exported(window, path) == original, "preview toggle did not restore source");
                    preview->setChecked(true);
                    require(exported(window, path) != original, "preview toggle did not restore preview");
                } catch (const std::exception &e) { previewError = e.what(); }
                dialog->reject();
            });
        });
        require(previewError.isEmpty(), qPrintable(previewError));
        require(exported(window, path) == original, "filter cancel altered source");
        modal(window, "filter.Gaussian Blur", [&](QDialog *dialog) {
            number(dialog, "radius")->setValue(3);
            click(dialog, QDialogButtonBox::Ok);
        });
        require(exported(window, path) != original, "filter commit missing");
        undo(window); require(exported(window, path) == original, "filter undo did not restore source in one step");
        // Regression: /qa 2026-09-21 — every Adjust sheet failed its first live preview
        // ("Invalid command JSON": the payload omitted `curves`), leaving OK disabled.
        for (const QString kind : {"Levels", "Hue/Saturation", "Curves", "Exposure", "Gradient Map", "Grain"}) {
            QString adjustError;
            modal(window, "adjust." + kind, [&](QDialog *dialog) {
                QTimer::singleShot(300, dialog, [&, dialog] {
                    auto *buttons = dialog->findChild<QDialogButtonBox *>();
                    if (!buttons || !buttons->button(QDialogButtonBox::Ok)->isEnabled()) adjustError = "OK disabled after first preview";
                    for (auto *label : dialog->findChildren<QLabel *>())
                        if (!label->text().isEmpty() && label->text().contains("failed")) adjustError = label->text();
                    dialog->reject();
                });
            });
            require(adjustError.isEmpty(), qPrintable(kind + ": " + adjustError));
            // Discard Cancelled New Adjustment Sheet (R26): rejection automatically rolled back the sheet
            require(exported(window, path) == original, "adjust discard did not restore source");
        }

        // Levels: Auto buttons and eyedropper sampling from the canvas.
        modal(window, "adjust.Levels", [&](QDialog *dialog) {
            for (int i = 0; i < 3; ++i) {
                auto *autoButton = dialog->findChild<QPushButton *>(QString("auto.%1").arg(i));
                require(autoButton && autoButton->isEnabled(), "Levels Auto button missing or disabled");
                autoButton->click(); QApplication::processEvents();
            }
            auto *sample = dialog->findChild<QPushButton *>("sample.0");
            require(sample && sample->isEnabled(), "Levels eyedropper missing or disabled");
            sample->click(); QApplication::processEvents();
            QWidget *canvas = window.centralWidget();
            const double scale = std::max(1, int(std::min((canvas->width() - 48) / 64.0, (canvas->height() - 48) / 64.0)));
            const QPointF origin((canvas->width() - 64 * scale) / 2.0, (canvas->height() - 64 * scale) / 2.0);
            const QPointF pos = origin + QPointF(28.5 * scale, 28.5 * scale);  // on the red stroke
            QMouseEvent press(QEvent::MouseButtonPress, pos, canvas->mapToGlobal(pos), Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
            QApplication::sendEvent(canvas, &press);
            QMouseEvent release(QEvent::MouseButtonRelease, pos, canvas->mapToGlobal(pos), Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
            QApplication::sendEvent(canvas, &release);
            QApplication::processEvents();
            require(dialog->isModal(), "dialog did not become modal again after sampling");
            auto *channel = dialog->findChild<QComboBox *>("channel");
            channel->setCurrentIndex(1);  // Red: sampled 255 sets the black point to 254
            require(number(dialog, "Black")->value() == 254, "black eyedropper did not calibrate the red channel");
            channel->setCurrentIndex(2);  // Green: sampled 0 keeps black at 0
            require(number(dialog, "Black")->value() == 0, "black eyedropper changed the green channel");
            click(dialog, QDialogButtonBox::Cancel);
        });
        require(exported(window, path) == original, "Levels sampling discard did not restore source");

        // Command Palette (Ctrl+Shift+P / F1)
        modal(window, "commandPalette", [&](QDialog *dialog) {
            auto *filter = dialog->findChild<QLineEdit *>("commandPalette.filter");
            auto *list = dialog->findChild<QListWidget *>("commandPalette.list");
            require(filter != nullptr, "command palette filter input missing");
            require(list != nullptr, "command palette list missing");
            require(list->count() > 0, "command palette list is empty");
            filter->setText("Canvas");
            require(list->count() > 0, "command palette search for 'Canvas' returned no items");
            dialog->reject();
        });

        modal(window, "imageSize", [&](QDialog *dialog) {
            dialog->findChild<QCheckBox *>("resample")->setChecked(false);
            number(dialog, "resolution")->setValue(300);
            click(dialog, QDialogButtonBox::Ok);
        });
        require(window.saveProject(temporary.filePath("project")), "save after dialog failed");
        require(window.loadProject(temporary.filePath("project")), "reopen after dialog failed");
        const QImage reopened = exported(window, path);
        require(reopened == original, "reopen after dialog changed pixels");
        require(qAbs(reopened.dotsPerMeterX() - qRound(300 / 0.0254)) <= 1, "reopened project lost export resolution");

        // Crash-Recovery Autosave (R61)
        window.clearAutosave();
        require(!window.hasAutosaveRecovery(), "unexpected autosave recovery state before edit");
        window.paintStroke(10, 10, 20, 20);
        require(window.performAutosave(), "performAutosave failed on dirty document");
        require(window.hasAutosaveRecovery(), "hasAutosaveRecovery false after performAutosave");
        require(window.recoverAutosave(), "recoverAutosave failed");
        window.clearAutosave();
        require(!window.hasAutosaveRecovery(), "hasAutosaveRecovery true after clearAutosave");
        // Platform seams: the shell runs entirely against injected fakes.
        {
            auto files = std::make_shared<FakeFiles>();
            auto clipboard = std::make_shared<FakeClipboard>();
            auto notifier = std::make_shared<FakeNotifier>();
            auto storage = std::make_shared<FakeStorage>();
            storage->root = temporary.filePath("appdata");
            PlatformServices services;
            services.files = files; services.clipboard = clipboard; services.notifier = notifier; services.storage = storage;
            SessionWindow injected(nullptr, services);
            auto action = [&](const QString &text) -> QAction * {
                for (QAction *a : injected.findChildren<QAction *>()) {
                    QString clean = a->text().remove('&');
                    if (clean == text) return a;
                    if (clean.replace(QChar(0x2026), "...") == text) return a;
                }
                require(false, "menu action missing"); return nullptr;
            };
            files->exportPath = temporary.filePath("injected.png");
            action("Export PNG...")->trigger();
            require(QFileInfo::exists(files->exportPath), "export did not use the injected file dialog");
            require(notifier->warnings.isEmpty(), "unexpected warning on a successful export");
            files->exportPath = temporary.filePath("missing-dir/injected.png");
            action("Export PNG...")->trigger();
            require(notifier->warnings.size() == 1, "failed export did not reach the injected notifier");
            action("Copy")->trigger();
            require(!clipboard->stored.isNull(), "copy did not reach the injected clipboard");
            require(injected.performAutosave(), "autosave failed against the injected storage");
            require(QDir(temporary.filePath("appdata/recovery")).exists(), "autosave ignored the injected storage locator");
        }
        documentSafetyJourney();
        qInfo("Qt dialog journey OK (resize, resolution, cancel, preview, commit, undo, command palette, autosave, save/reopen)");
        return 0;
    } catch (const std::exception &e) {
        qCritical("Qt dialog journey failed: %s", e.what()); return 1;
    }
}

// Count non-transparent pixels with a strong channel, i.e. pixels the compositor
// actually painted (as opposed to the transparent canvas background).
static int countPainted(const QImage &image) {
    int colored = 0;
    for (int y = 0; y < image.height(); ++y) {
        for (int x = 0; x < image.width(); ++x) {
            const QColor pixel = image.pixelColor(x, y);
            if (pixel.alpha() > 0 && (pixel.red() > 200 || pixel.green() > 200 || pixel.blue() > 200)) ++colored;
        }
    }
    return colored;
}

// Brush palette + blend round-trip: the palette sliders and color button feed
// brushBegin parameters through the C ABI, the blend combo drives setBlendMode,
// and the painted stroke renders with the chosen color/size.
extern "C" int compositor_host_brush_smoke(int argc, char **argv) {
    QApplication app(argc, argv);
    try {
        QTemporaryDir temporary;
        require(temporary.isValid(), "temporary directory failed");
        SessionWindow window; window.show(); QApplication::processEvents();
        auto *diameter = window.findChild<QSlider *>("brush.diameter");
        auto *hardness = window.findChild<QSlider *>("brush.hardness");
        auto *opacity = window.findChild<QSlider *>("brush.opacity");
        auto *blend = window.findChild<QComboBox *>("blend.mode");
        require(diameter && hardness && opacity && blend, "brush palette controls missing");
        diameter->setValue(24);
        hardness->setValue(100);
        opacity->setValue(100);
        blend->setCurrentText("Multiply");
        const QJsonArray layers = window.sessionState().value("layers").toArray();
        require(layers.size() > 0 && layers.at(0).toObject().value("blendMode").toString() == "Multiply", "blend combo did not reach setBlendMode");

        window.paintStroke(8, 8, 40, 40);
        const QImage painted = exported(window, temporary.filePath("brush.png"));
        require(countPainted(painted) > 0, "palette stroke did not paint");

        // Clone Stamp: clone the opaque diagonal stroke onto a still-blank corner (source-over onto transparent is
        // a real, visible change, unlike cloning from transparent onto opaque — which upstream correctly no-ops).
        window.setTool(SessionWindow::Tool::CloneStamp);
        window.cloneStroke(58, 58, 58, 58);
        const QImage beforeSource = exported(window, temporary.filePath("clone-before.png"));
        require(qAlpha(beforeSource.pixel(58, 58)) == 0, "a clone stroke with no source set changed the image");
        window.setCloneSource(20, 20);
        window.cloneStroke(58, 58, 58, 58);
        const QImage cloned = exported(window, temporary.filePath("clone-after.png"));
        require(qAlpha(cloned.pixel(58, 58)) > 0, "clone stroke did not carry the opaque source's pixels");

        // Spot Healing: any stroke should change the pixels it covers.
        window.setTool(SessionWindow::Tool::SpotHealing);
        window.healStroke(20, 20, 24, 20);
        const QImage healed = exported(window, temporary.filePath("healed.png"));
        require(healed != cloned, "spot healing stroke did not paint");

        auto *visible = window.findChild<QCheckBox *>("layer.visible");
        require(visible, "visibility checkbox missing");
        visible->setChecked(false); QApplication::processEvents();
        require(!window.sessionState().value("layers").toArray().at(0).toObject().value("visible").toBool(true), "visibility toggle did not reach setVisible");
        visible->setChecked(true); QApplication::processEvents();
        require(window.sessionState().value("layers").toArray().at(0).toObject().value("visible").toBool(false) != false, "visibility re-enable did not reach setVisible");

        auto *selectAll = window.findChild<QAction *>("select.rectangle");
        auto *fill = window.findChild<QAction *>("fill.foreground");
        require(selectAll && fill, "select/fill actions missing");
        selectAll->trigger(); QApplication::processEvents();
        fill->trigger(); QApplication::processEvents();
        const QImage filled = exported(window, temporary.filePath("filled.png"));
        require(countPainted(filled) > 0, "selection fill did not paint");

        qInfo("Qt brush palette journey OK (diameter/hardness/opacity, color, blend, paint, visible, select, fill, clone, heal)");
        return 0;
    } catch (const std::exception &e) {
        qCritical("Qt brush palette journey failed: %s", e.what()); return 1;
    }
}

// Layers dock journey: the QTreeView hierarchical model mirrors the Swift state JSON, and the
// Layer menu actions round-trip addLayer/duplicateLayer/deleteLayer/selectLayer/
// setOpacity/addGroup/masks through the C ABI.
extern "C" int compositor_host_layers_smoke(int argc, char **argv) {
    QApplication app(argc, argv);
    try {
        QTemporaryDir temporary;
        require(temporary.isValid(), "temporary directory failed");
        SessionWindow window; window.show(); QApplication::processEvents();
        auto *tree = window.findChild<QTreeView *>();
        require(tree != nullptr, "layers tree view missing");
        auto *model = qobject_cast<QStandardItemModel *>(tree->model());
        require(model != nullptr, "layers model missing");
        auto *opacity = window.findChild<QSlider *>();
        require(opacity != nullptr, "opacity slider missing");
        auto menuAction = [&](const QString &text) -> QAction * {
            for (QAction *action : window.findChildren<QAction *>()) {
                QString t = action->text().remove('&');
                if (t == text) return action;
                if (t.replace(QChar(0x2026), "...") == text) return action;
                if (text == "New Layer" && (t == "New Blank Layer" || t == "New Layer")) return action;
                if (text == "New Folder / Group" && (t == "Group Selected Layers" || t == "New Folder / Group")) return action;
                if (text == "Add Reveal Mask" && (t == "Add Reveal Mask" || t == "Reveal All")) return action;
            }
            return nullptr;
        };
        const auto stateLayers = [&]() -> QJsonArray {
            return window.sessionState().value("layers").toArray();
        };

        auto countItems = [&](auto self, const QModelIndex &parent = QModelIndex()) -> int {
            int total = 0;
            const int rows = model->rowCount(parent);
            total += rows;
            for (int r = 0; r < rows; ++r) {
                total += self(self, model->index(r, 0, parent));
            }
            return total;
        };

        require(countItems(countItems) == stateLayers().size(), "dock rows do not match session layers");
        const int opened = countItems(countItems);
        require(opened == 1, "initial dock does not hold the brush layer");
        const QImage original = exported(window, temporary.filePath("view.png"));

        auto *add = menuAction("New Layer"); require(add, "New Layer action missing");
        auto *duplicate = menuAction("Duplicate Layer"); require(duplicate, "Duplicate Layer action missing");
        auto *remove = menuAction("Delete Layer"); require(remove, "Delete Layer action missing");
        add->trigger();
        require(stateLayers().size() == 2 && countItems(countItems) == 2, "New Layer did not add a dock row");
        // The dock lists the top layer first; the document array is bottom-first.
        require(tree->currentIndex().row() == 0 && window.sessionState().value("activeLayerID").toString()
            == stateLayers().at(stateLayers().size() - 1 - tree->currentIndex().row()).toObject().value("id").toString(), "new layer not selected in the dock");
        duplicate->trigger();
        require(stateLayers().size() == 3 && countItems(countItems) == 3, "duplicate did not add a dock row");
        tree->setCurrentIndex(model->index(1, 0));
        require(window.sessionState().value("activeLayerID").toString()
            == stateLayers().at(stateLayers().size() - 2).toObject().value("id").toString(), "row selection did not switch active layer");
        opacity->setValue(50);
        const QString active = window.sessionState().value("activeLayerID").toString();
        double set = -1;
        for (const QJsonValue &v : stateLayers()) {
            if (v.toObject().value("id").toString() == active) set = v.toObject().value("opacity").toDouble(-1);
        }
        require(set > 0.49 && set < 0.51, "opacity slider did not round-trip");
        remove->trigger();
        require(stateLayers().size() == 2 && countItems(countItems) == 2, "delete did not remove a dock row");

        // Multi-selection layer deletion (ExtendedSelection)
        add->trigger();
        add->trigger();
        const int beforeMulti = countItems(countItems);
        require(beforeMulti >= 4, "failed to add layers for multi-selection test");
        tree->selectionModel()->clearSelection();
        tree->selectionModel()->select(model->index(0, 0), QItemSelectionModel::Select | QItemSelectionModel::Rows);
        tree->selectionModel()->select(model->index(1, 0), QItemSelectionModel::Select | QItemSelectionModel::Rows);
        require(tree->selectionModel()->selectedRows().size() == 2, "failed to select two rows");
        remove->trigger();
        require(countItems(countItems) == beforeMulti - 2, "multi-selection delete failed to delete both layers");

        // Group / Folder hierarchical verification
        auto *addGroup = menuAction("New Folder / Group");
        require(addGroup != nullptr, "New Folder / Group action missing");
        addGroup->trigger();
        require(countItems(countItems) == stateLayers().size(), "New Folder did not add a group row");
        const QString grpActive = window.sessionState().value("activeLayerID").toString();
        bool isGroup = false;
        for (const QJsonValue &v : stateLayers()) {
            if (v.toObject().value("id").toString() == grpActive) isGroup = v.toObject().value("isGroup").toBool();
        }
        require(isGroup, "active layer is not a group");

        // Mask verification on a raster layer
        tree->setCurrentIndex(model->index(model->rowCount() - 1, 0));  // bottom raster layer
        auto *addMask = menuAction("Add Reveal Mask");
        require(addMask != nullptr, "Add Reveal Mask action missing");
        addMask->trigger();
        const QString maskActive = window.sessionState().value("activeLayerID").toString();
        bool hasMask = false;
        for (const QJsonValue &v : stateLayers()) {
            if (v.toObject().value("id").toString() == maskActive) hasMask = v.toObject().value("hasMask").toBool();
        }
        require(hasMask, "layer hasMask is false after Add Reveal Mask");

        require(!exported(window, temporary.filePath("after.png")).isNull(), "render after layer ops failed");
        // Move / Transform: X/Y/W/H fields, Link, handle resize, body drag, panel order.
        {
            SessionWindow w2; w2.resize(1200, 800); w2.show(); QApplication::processEvents();
            w2.setTool(SessionWindow::Tool::Move); QApplication::processEvents();
            auto spin = [&](const char *name) { auto *s = w2.findChild<QSpinBox *>(name); require(s, "transform spin missing"); return s; };
            const auto geometry = [&]() {
                const QJsonObject layer = w2.sessionState().value("layers").toArray().at(0).toObject();
                const QJsonObject t = layer.value("transform").toObject();
                auto pair = [](const QJsonValue &v, const char *a, const char *b) {
                    if (v.isArray()) return QPointF(v.toArray().at(0).toDouble(), v.toArray().at(1).toDouble());
                    return QPointF(v.toObject().value(a).toDouble(), v.toObject().value(b).toDouble());
                };
                const QPointF o = pair(t.value("origin"), "x", "y"), z = pair(t.value("size"), "width", "height");
                return QRectF(o.x(), o.y(), z.x(), z.y());
            };
            // The startup brush stroke leaves a 56x56 layer inside the 64x64 document.
            require(geometry().size() == QSizeF(56, 56), "unexpected initial layer size");
            require(spin("transform.w")->value() == 56 && spin("transform.h")->value() == 56, "W/H fields do not mirror the layer");
            spin("transform.w")->setValue(32); QApplication::processEvents();
            require(geometry().size() == QSizeF(32, 32), "linked W edit did not resize the layer to 32x32");
            require(spin("transform.h")->value() == 32, "linked H field did not follow W");
            spin("transform.x")->setValue(6); QApplication::processEvents();
            require(qRound(geometry().x()) == 6, "X field did not move the layer");
            spin("transform.x")->setValue(0); QApplication::processEvents();

            QWidget *canvas = w2.centralWidget();
            const double scale = std::max(1, int(std::min((canvas->width() - 48) / 64.0, (canvas->height() - 48) / 64.0)));
            const QPointF origin((canvas->width() - 64 * scale) / 2.0, (canvas->height() - 64 * scale) / 2.0);
            auto at = [&](double dx, double dy) { return origin + QPointF(dx * scale, dy * scale); };
            auto drag = [&](QPointF from, QPointF to) {
                auto send = [&](QEvent::Type type, QPointF pos, Qt::MouseButton button, Qt::MouseButtons buttons) {
                    QMouseEvent ev(type, pos, canvas->mapToGlobal(pos), button, buttons, Qt::NoModifier);
                    QApplication::sendEvent(canvas, &ev);
                };
                send(QEvent::MouseButtonPress, from, Qt::LeftButton, Qt::LeftButton);
                send(QEvent::MouseMove, (from + to) / 2, Qt::NoButton, Qt::LeftButton);
                send(QEvent::MouseMove, to, Qt::NoButton, Qt::LeftButton);
                send(QEvent::MouseButtonRelease, to, Qt::LeftButton, Qt::NoButton);
                QApplication::processEvents();
            };
            drag(at(32, 32), at(48, 40));  // bottom-right handle, linked => uniform 1.5x
            require(geometry().size() == QSizeF(48, 48), "corner handle drag did not resize uniformly");
            require(spin("transform.w")->value() == 48, "fields did not refresh after handle drag");
            drag(at(20, 20), at(24, 25));  // body drag
            require(qRound(geometry().x()) == 4 && qRound(geometry().y()) == 5, "body drag did not move the layer");
            require(window.sessionState().value("canUndo").toBool(), "transform edits left no history entry");
        }

        // Selection tools: New/Add combine modes, Expand, polygonal lasso.
        {
            SessionWindow w3; w3.resize(1200, 800); w3.show(); QApplication::processEvents();
            QWidget *canvas = w3.centralWidget();
            const double scale = std::max(1, int(std::min((canvas->width() - 48) / 64.0, (canvas->height() - 48) / 64.0)));
            const QPointF origin((canvas->width() - 64 * scale) / 2.0, (canvas->height() - 64 * scale) / 2.0);
            auto at = [&](double dx, double dy) { return origin + QPointF(dx * scale, dy * scale); };
            auto send = [&](QEvent::Type type, QPointF pos, Qt::MouseButton button, Qt::MouseButtons buttons) {
                QMouseEvent ev(type, pos, canvas->mapToGlobal(pos), button, buttons, Qt::NoModifier);
                QApplication::sendEvent(canvas, &ev);
            };
            auto drag = [&](QPointF from, QPointF to) {
                send(QEvent::MouseButtonPress, from, Qt::LeftButton, Qt::LeftButton);
                send(QEvent::MouseMove, (from + to) / 2, Qt::NoButton, Qt::LeftButton);
                send(QEvent::MouseMove, to, Qt::NoButton, Qt::LeftButton);
                send(QEvent::MouseButtonRelease, to, Qt::LeftButton, Qt::NoButton);
                QApplication::processEvents();
            };
            auto click = [&](QPointF pos) {
                send(QEvent::MouseButtonPress, pos, Qt::LeftButton, Qt::LeftButton);
                send(QEvent::MouseButtonRelease, pos, Qt::LeftButton, Qt::NoButton);
                QApplication::processEvents();
            };
            auto settle = [] { for (int i = 0; i < 5; ++i) { QApplication::processEvents(); QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete); } };
            auto header = [&] { settle(); return w3.findChild<QWidget *>("swiftUIOptionsContainer"); };
            auto button = [&](const QString &text) -> QPushButton * {
                for (auto *b : header()->findChildren<QPushButton *>()) if (b->text() == text && b->isVisible()) return b;
                require(false, "visible shared options button missing"); return nullptr;
            };
            auto pick = [&](const QString &text) {
                for (auto *combo : header()->findChildren<QComboBox *>()) if (combo->findText(text) >= 0 && combo->isVisible()) {
                    combo->setCurrentText(text); settle(); return;
                }
                require(false, "visible selection picker missing");
            };
            QImage baseline = exported(w3, temporary.filePath("baseline.png"));
            auto fillAndExport = [&](const char *file) {
                baseline = exported(w3, temporary.filePath("baseline.png"));
                for (QAction *action : w3.findChildren<QAction *>()) if (action->objectName() == "fill.foreground") action->trigger();
                QApplication::processEvents();
                return exported(w3, temporary.filePath(file));
            };
            // A pixel counts as filled when the fill changed it relative to the pre-fill render.
            auto filled = [&](const QImage &img, int x, int y) { return img.pixelColor(x, y) != baseline.pixelColor(x, y); };

            w3.setTool(SessionWindow::Tool::RectSelect); QApplication::processEvents();
            drag(at(30, 4), at(44, 18));
            pick("Add");
            drag(at(4, 34), at(18, 48));
            const QImage combined = fillAndExport("combined.png");
            require(filled(combined, 36, 10) && filled(combined, 10, 40), "Add mode did not keep both rectangles");
            require(!filled(combined, 30, 30) && !filled(combined, 22, 24), "Add mode filled the gap between rectangles");
            pick("New");

            // Expand: a fresh 8px square grows by 6px on each side.
            w3.setTool(SessionWindow::Tool::RectSelect); QApplication::processEvents();
            drag(at(50, 4), at(58, 12));   // 8x8 at (50,4)
            auto fields = header()->findChildren<QLineEdit *>();
            require(!fields.isEmpty() && fields.first()->isVisible(), "selection expand amount missing");
            fields.first()->setText("6"); settle();



            button("Expand")->click(); QApplication::processEvents();
            const QImage grown = fillAndExport("grown.png");
            require(filled(grown, 46, 8) && filled(grown, 55, 8), "Expand did not grow the selection");

            // Polygonal lasso: three clicks and a click on the first vertex close the triangle.
            w3.setTool(SessionWindow::Tool::Lasso); QApplication::processEvents();
            pick("Polygonal");
            click(at(44, 30)); click(at(54, 30)); click(at(54, 54)); click(at(44, 30));
            const QImage polygon = fillAndExport("polygon.png");
            require(filled(polygon, 52, 40), "polygonal lasso selection was not applied");
        }

        qInfo("Qt layers dock journey OK (add, duplicate, select, opacity, delete, multi-delete, group, mask)");
        return 0;
    } catch (const std::exception &e) {
        qCritical("Qt layers dock journey failed: %s", e.what()); return 1;
    }
}

// Visible shared controls: real keyboard/pointer delivery, Swift dispatch,
// pixel output, undo, and window lifetime. No user files or desktop input.
// Real Qt interactions for the newly connected Linux paths.
static void integratedUIJourney() {
    auto pump=[] {for(int i=0;i<5;++i) {QApplication::processEvents();QCoreApplication::sendPostedEvents(nullptr,QEvent::DeferredDelete);}};
    auto key=[&](QWidget *target,int code,Qt::KeyboardModifiers mods=Qt::NoModifier) {
        QKeyEvent p(QEvent::KeyPress,code,mods,code<0x10000 ? QString(QChar(code)) : QString()); QApplication::sendEvent(target,&p);
        QKeyEvent r(QEvent::KeyRelease,code,mods); QApplication::sendEvent(target,&r); pump();
    };
    SessionWindow w; w.show(); w.activateWindow(); w.createNewDocument(64,64); pump();
    auto *canvas=w.findChild<QWidget *>("editor.canvas");
    auto *tree=w.findChild<QTreeView *>("layers.treeView");
    auto act=[&](const char *name) {auto *a=w.findChild<QAction *>(name); require(a,"missing UI action"); a->trigger();pump();};
    auto header=[&] {return w.findChild<QWidget *>("swiftUIOptionsContainer");};
    auto drag=[&](QPointF from,QPointF to,Qt::KeyboardModifiers mods=Qt::NoModifier) {
        canvas->setFocus();
        QMouseEvent p(QEvent::MouseButtonPress,from,canvas->mapToGlobal(from.toPoint()),Qt::LeftButton,Qt::LeftButton,mods); QApplication::sendEvent(canvas,&p);
        QMouseEvent m(QEvent::MouseMove,to,canvas->mapToGlobal(to.toPoint()),Qt::NoButton,Qt::LeftButton,mods); QApplication::sendEvent(canvas,&m);
        QMouseEvent r(QEvent::MouseButtonRelease,to,canvas->mapToGlobal(to.toPoint()),Qt::LeftButton,Qt::NoButton,mods); QApplication::sendEvent(canvas,&r); pump();
    };
    modal(w,"edit.shortcuts",[&](QDialog *d) {
        auto *field=d->findChild<QKeySequenceEdit *>("Canvas & Layers:Brush tool"); require(field,"shortcut recorder missing");
        field->setKeySequence(QKeySequence(Qt::Key_V)); click(d,QDialogButtonBox::Save);
        require(d->isVisible() && !d->findChild<QLabel *>("shortcuts.error")->text().isEmpty(),"conflicting shortcut was accepted");
        field->clear(); field->setFocus(); key(field,Qt::Key_K);
        require(field->keySequence()==QKeySequence(Qt::Key_K),"recorder did not capture key event");
        click(d,QDialogButtonBox::Save);
    }); pump();
    QProcess readback;
    auto environment=QProcessEnvironment::systemEnvironment(); environment.insert("COMPOSITOR_CHECK_SHORTCUT","k");
    readback.setProcessEnvironment(environment);
    readback.start(QCoreApplication::applicationFilePath(),{"--ui-smoke"});
    const bool finished=readback.waitForFinished(15000);
    if(!finished || readback.exitCode()!=0) qWarning().noquote()<<readback.readAllStandardError();
    require(finished && readback.exitCode()==0,"shortcut did not survive a process restart");
    w.setTool(SessionWindow::Tool::Move); canvas->setFocus(); key(canvas,Qt::Key_K);
    require(w.currentTool()==SessionWindow::Tool::Brush,"recorded shortcut did not select brush");
    w.setTool(SessionWindow::Tool::Move); canvas->setFocus(); key(canvas,Qt::Key_B);
    require(w.currentTool()==SessionWindow::Tool::Move,"old shortcut was not suppressed");
    modal(w,"edit.shortcuts",[&](QDialog *d) {click(d,QDialogButtonBox::RestoreDefaults);click(d,QDialogButtonBox::Save);});pump();
    canvas->setFocus(); key(canvas,Qt::Key_B); require(w.currentTool()==SessionWindow::Tool::Brush,"restore default shortcut failed");
    qInfo("Shortcut integration passed");
    modal(w,"edit.shortcuts",[&](QDialog *d) {
        d->findChild<QKeySequenceEdit *>("Canvas & Layers:Brush tool")->setKeySequence(QKeySequence(Qt::Key_K));
        click(d,QDialogButtonBox::Cancel);
    });pump();
    for(auto v:w.sessionState().value("shortcuts").toArray()) {auto d=v.toObject();if(d.value("title")=="Brush tool") require(d.value("key")=="b","cancel saved shortcut draft");}
    // Shared swatch must open the actual native picker; cancellation is lossless.
    auto swatch=[&]() -> QPushButton * {
        for(auto *button:header()->findChildren<QPushButton *>()) if(button->isVisible() && button->toolTip().contains("color",Qt::CaseInsensitive)) return button;
        return nullptr;
    };
    auto *button=swatch(); require(button,"shared swatch missing");
    const auto original=w.sessionState().value("palette");
    bool pickerVisited=false;
    QTimer::singleShot(0,&w,[&] {QTimer::singleShot(0,&w,[&] {
        auto *d=qobject_cast<QDialog *>(QApplication::activeModalWidget());
        if(d && d->findChild<QLineEdit *>("colorPicker.hex")) {pickerVisited=true; d->reject();}
    });});
    button->click();pump();
    require(pickerVisited,"shared color action did not present picker");
    require(w.sessionState().value("palette")==original,"cancel changed palette");
    pickerVisited=false;
    QTimer::singleShot(0,&w,[&] {QTimer::singleShot(0,&w,[&] {
        auto *d=qobject_cast<QDialog *>(QApplication::activeModalWidget()); if(!d) return;
        auto *hex=d->findChild<QLineEdit *>("colorPicker.hex"); if(!hex) {d->reject();return;}
        hex->setText("12AB34"); QMetaObject::invokeMethod(hex,"editingFinished",Qt::DirectConnection);pickerVisited=true;click(d,QDialogButtonBox::Ok);
    });});
    swatch()->click();pump();
    require(pickerVisited,"color commit dialog missing");
    const auto palette=w.sessionState().value("palette").toObject();
    require(qRound(255*palette.value("green").toDouble())==171,"picker color not committed to shared palette");
    require(swatch()->styleSheet().contains("#12ab34"),"shared swatch does not display current color");
    auto *railColor=w.findChild<QWidget *>("palette.controls")->findChild<QPushButton *>("brush.color");
    require(railColor && railColor->isVisible() && railColor->styleSheet().contains("#12ab34"),"rail swatch is stale or hidden");
    qInfo("Color picker integration passed");
    // Real tree drop events exercise native hit testing and the model's MIME route.
    const auto first=w.sessionState().value("activeLayerID").toString(); act("layer.new");
    const auto second=w.sessionState().value("activeLayerID").toString();
    act("layer.addGroup"); const auto folder=w.sessionState().value("activeLayerID").toString();
    auto indexFor=[&](const QString &id) {
        std::function<QModelIndex(QModelIndex)> visit=[&](QModelIndex parent) -> QModelIndex {
            for(int row=0;row<tree->model()->rowCount(parent);++row) {
                auto index=tree->model()->index(row,0,parent); if(index.data(Qt::UserRole).toString()==id) return index;
                auto found=visit(index);if(found.isValid()) return found;
            } return {};
        }; return visit({});
    };
    auto *model=dynamic_cast<LayerDragModel *>(tree->model());require(model,"drag model missing");
    // Group action wraps selected second layer. Drag first into that group.
    auto from=indexFor(first), target=indexFor(folder);require(from.isValid()&&target.isValid(),"drop rows missing");
    std::unique_ptr<QMimeData> mime(model->mimeData({from}));
    const QPoint pos=tree->visualRect(target).center();
    QDragEnterEvent enter(pos,Qt::MoveAction,mime.get(),Qt::LeftButton,Qt::NoModifier);QApplication::sendEvent(tree->viewport(),&enter);
    QDragMoveEvent moving(pos,Qt::MoveAction,mime.get(),Qt::LeftButton,Qt::NoModifier);QApplication::sendEvent(tree->viewport(),&moving);
    QDropEvent drop(pos,Qt::MoveAction,mime.get(),Qt::LeftButton,Qt::NoModifier);QApplication::sendEvent(tree->viewport(),&drop);pump();
    require(drop.isAccepted(),"tree rejected valid native drop");
    auto parentOf=[&](const QString &id) {for(auto v:w.sessionState().value("layers").toArray()) {auto l=v.toObject();if(l.value("id")==id)return l.value("parentID").toString();}return QString();};
    require(parentOf(first)==folder,"drop did not nest layer");
    std::unique_ptr<QMimeData> cycle(model->mimeData({indexFor(folder)}));
    require(!model->canDropMimeData(cycle.get(),Qt::MoveAction,-1,0,indexFor(folder)),"folder can drop into itself");
    undo(w);pump();require(parentOf(first).isEmpty(),"layer drop did not undo");
    // Context menu routes to existing action enablement.
    bool contextVisited=false;
    QTimer::singleShot(0,&w,[&] {auto *menu=qobject_cast<QMenu *>(QApplication::activePopupWidget());if(menu){contextVisited=menu->actions().size()>=5;menu->close();}});
    const QPoint menuPoint=tree->visualRect(indexFor(first)).center();
    QContextMenuEvent menuEvent(QContextMenuEvent::Mouse,menuPoint,tree->viewport()->mapToGlobal(menuPoint));
    QApplication::sendEvent(tree->viewport(),&menuEvent);
    require(contextVisited,"layer context menu missing");
    qInfo("Layer integration passed");
    w.createNewDocument(64,64);w.setTool(SessionWindow::Tool::Shape);pump();
    auto *shape=header()->findChild<QComboBox *>();require(shape,"shape picker missing");shape->setCurrentIndex(1);pump();
    const QPointF center=canvas->rect().center(); const int before=w.sessionState().value("layers").toArray().size();
    drag(center-QPointF(12,12),center+QPointF(12,12));
    require(w.sessionState().value("layers").toArray().size()==before+1,"shape gesture did not create layer");
    require(w.sessionState().value("undoName")=="Ellipse","shape gesture ignored shared picker");
    undo(w);pump(); require(w.sessionState().value("layers").toArray().size()==before,"shape undo failed");
    canvas->setFocus();key(canvas,Qt::Key_Tab);
    require(w.shapeMode()==SessionWindow::ShapeMode::Line,"canvas Tab did not cycle the tool mode");
    w.setTool(SessionWindow::Tool::Gradient);pump();drag(center-QPointF(15,0),center+QPointF(15,0));
    QTemporaryDir tmp;
    auto preview=exported(w,tmp.filePath("preview.png")); require(qAlpha(preview.pixel(32,32))>0,"gradient preview has no pixels");
    canvas->setFocus();key(canvas,Qt::Key_Escape);require(qAlpha(exported(w,tmp.filePath("cancel.png")).pixel(32,32))==0,"gradient cancel left pixels");
    drag(center-QPointF(15,0),center+QPointF(15,0)); canvas->setFocus();key(canvas,Qt::Key_Return);
    require(w.sessionState().value("undoName")=="Gradient","gradient apply not recorded");undo(w);pump();
    require(qAlpha(exported(w,tmp.filePath("undo.png")).pixel(32,32))==0,"gradient undo left pixels");
    w.setTool(SessionWindow::Tool::Crop);pump();auto *ratio=header()->findChild<QComboBox *>();require(ratio,"crop ratio picker missing");ratio->setCurrentIndex(4);pump();
    auto crop=w.sessionState().value("cropRect").toArray();require(crop.size()==4 && crop[3].toInt()==36,"crop ratio setting not integrated");
    w.createNewDocument(256,128);w.setTool(SessionWindow::Tool::Type);pump();
    bool foundSize=false;
    for(auto *field:header()->findChildren<QLineEdit *>()) if(field->accessibleName()=="Size") {field->setText("12");foundSize=true;}
    require(foundSize,"type size field inaccessible");pump();
    bool textVisited=false;
    QTimer::singleShot(0,&w,[&] {
        auto *d=qobject_cast<QDialog *>(QApplication::activeModalWidget());if(!d)return;
        auto *text=d->findChild<QTextEdit *>("text.content");if(!text){d->reject();return;}
        text->setPlainText("Linux");textVisited=true;click(d,QDialogButtonBox::Ok);
    });
    const int textBefore=w.sessionState().value("layers").toArray().size();
    drag(center,center);
    require(textVisited && w.sessionState().value("layers").toArray().size()==textBefore+1,"type tool could not create editable text");
    require(w.saveProject(tmp.filePath("text.comp")),"text fixture save failed");
    QFile manifest(tmp.filePath("text.comp/manifest.json"));require(manifest.open(QIODevice::ReadOnly),"text manifest missing");
    bool sizeApplied=false;
    for(auto v:QJsonDocument::fromJson(manifest.readAll()).object().value("layers").toArray())
        if(v.toObject().value("text").toObject().value("fontSize").toInt()==12) sizeApplied=true;
    require(sizeApplied,"type gesture ignored the shared size setting");
    const QString evidence=qEnvironmentVariable("COMPOSITOR_UI_EVIDENCE");
    if(!evidence.isEmpty()) {w.setTool(SessionWindow::Tool::Shape);pump();require(w.grab().save(evidence+"/integrated-controls.png"),"integration screenshot failed");}
    qInfo("Qt integration journey OK (record/conflict/reset shortcuts, picker cancel/commit/swatch, native layer drop/undo/cycle/context menu, shape/gradient pixels, crop ratio)");
}

extern "C" int compositor_host_ui_smoke(int argc, char **argv) {
    QApplication app(argc, argv);
    try {
        if(!qEnvironmentVariableIsEmpty("COMPOSITOR_CHECK_SHORTCUT")) {
            SessionWindow check;
            for(auto v:check.sessionState().value("shortcuts").toArray()) {auto d=v.toObject();
                if(d.value("title")=="Brush tool") return d.value("key").toString()==qEnvironmentVariable("COMPOSITOR_CHECK_SHORTCUT") ? 0 : 1;
            }
            return 1;
        }

        QTemporaryDir temporary;
        require(temporary.isValid(), "temporary directory failed");
        auto pump = [] { for (int i = 0; i < 5; ++i) { QApplication::processEvents(); QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete); } };
        auto key = [&](QWidget *target, int code, const QString &text, Qt::KeyboardModifiers mods = Qt::NoModifier) {
            QKeyEvent press(QEvent::KeyPress, code, mods, text);
            QApplication::sendEvent(target, &press);
            QKeyEvent release(QEvent::KeyRelease, code, mods, text);
            QApplication::sendEvent(target, &release);
            pump();
        };
        SessionWindow window; window.show(); window.activateWindow(); pump();
        window.createNewDocument(64, 64);
        window.setTool(SessionWindow::Tool::Brush); pump();
        auto header = [&] { return window.findChild<QWidget *>("swiftUIOptionsContainer"); };
        require(header() && header()->isVisible(), "shared header is not visible");
        auto fields = header()->findChildren<QLineEdit *>();
        require(fields.size() >= 4, "shared brush fields missing");
        QPointer<QLineEdit> size(fields[0]);
        require(size->isVisible() && !size->visibleRegion().isEmpty(), "size control is hidden in toolbar overflow");
        require(!window.findChild<QSlider *>("brush.diameter")->isVisible(), "hidden fallback toolbar reappeared");
        size->setFocus(); size->selectAll();
        key(size, Qt::Key_2, "2");
        require(size && size->hasFocus(), "typing deleted the editor or lost focus");
        key(size, Qt::Key_4, "4");
        require(size && size->text() == "24", "multi-character size edit was interrupted");
        require(window.sessionState().value("brush").toObject().value("diameter").toInt() == 24, "visible size did not reach session");
        auto *mode = header()->findChild<QComboBox *>();
        require(mode && mode->count() == 2, "brush mode options missing");
        key(mode, Qt::Key_Down, "");
        require(window.sessionState().value("brush").toObject().value("erasing").toInt() == 1, "picker did not reach typed binding");
        key(mode, Qt::Key_Up, "");
        auto sliders = header()->findChildren<QSlider *>();
        require(sliders.size() >= 3, "shared sliders missing");
        QPointer<QSlider> hardness(sliders[0]); hardness->setFocus();
        key(hardness, Qt::Key_Left, "");
        require(hardness && window.sessionState().value("brush").toObject().value("hardness").toDouble() < 1, "slider key did not change hardness");
        hardness->setValue(1000); pump();
        // Smoothing used to reset at every brushBegin even after changing its UI.
        sliders[2]->setValue(150); pump();
        auto *canvas = window.findChild<QWidget *>("editor.canvas");
        require(canvas && canvas->isVisible(), "canvas missing");
        canvas->setFocus(); pump();
        const QPointF point = canvas->rect().center();
        QMouseEvent press(QEvent::MouseButtonPress, point, canvas->mapToGlobal(point.toPoint()), Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
        QApplication::sendEvent(canvas, &press);
        QMouseEvent release(QEvent::MouseButtonRelease, point, canvas->mapToGlobal(point.toPoint()), Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
        QApplication::sendEvent(canvas, &release); pump();
        require(window.sessionState().value("brush").toObject().value("diameter").toInt() == 24, "pointer input overwrote the visible size");
        require(window.sessionState().value("brush").toObject().value("smoothing").toInt() == 15, "pointer input overwrote smoothing");
        const QImage painted = exported(window, temporary.filePath("painted.png"));
        require(qAlpha(painted.pixel(32, 32)) > 0, "real pointer click did not paint canvas center");
        undo(window); pump();
        const QImage undone = exported(window, temporary.filePath("undone.png"));
        require(qAlpha(undone.pixel(32, 32)) == 0, "pointer stroke did not undo in one step");
        // The Linux responder equivalent of the upstream NSTableView test.
        auto *tree = window.findChild<QTreeView *>(); require(tree, "layers tree missing");
        tree->setFocus();
        key(tree, Qt::Key_BracketRight, "]");
        require(window.sessionState().value("brush").toObject().value("diameter").toInt() > 24, "bracket key lost at layers view");
        key(tree, Qt::Key_BraceLeft, "{", Qt::ShiftModifier);
        require(window.sessionState().value("brush").toObject().value("hardness").toDouble() == 0.75, "hardness did not use Mac 25-percent steps");
        const int diameter = window.sessionState().value("brush").toObject().value("diameter").toInt();
        size->setFocus(); size->selectAll(); key(size, Qt::Key_BracketRight, "]");
        require(size && size->text() == "]", "text editor swallowed bracket input");
        require(window.sessionState().value("brush").toObject().value("diameter").toInt() == diameter, "typing changed the brush shortcut state");
        // Queue a notification then destroy its receiver. A second window still works.
        {
            auto other = std::make_unique<SessionWindow>(); other->show(); other->setTool(SessionWindow::Tool::Brush); pump();
            auto *container = other->findChild<QWidget *>("swiftUIOptionsContainer");
            auto *field = container->findChild<QLineEdit *>(); require(field, "second window editor missing");
            field->setText("37");
        }
        pump(); size->setText("31"); pump();
        require(window.sessionState().value("brush").toObject().value("diameter").toInt() == 31, "live window dispatch failed after closing another window");
        window.setTool(SessionWindow::Tool::Move); pump(); tree->setFocus();
        key(tree, Qt::Key_BracketRight, "]");
        require(window.sessionState().value("brush").toObject().value("diameter").toInt() == 31, "non-brush tool changed brush settings");
        window.setTool(SessionWindow::Tool::Brush); pump();
        const QString output = qEnvironmentVariable("COMPOSITOR_UI_EVIDENCE");
        if (!output.isEmpty()) {
            QDir().mkpath(output);
            window.paintStroke(12, 20, 48, 40); pump();
            require(window.grab().save(output + "/brush-controls.png"), "fixture screenshot failed");
            require(window.saveProject(output + "/linux-ui-fixture.comp"), "interchange fixture save failed");
            require(window.exportPNG(output + "/linux-ui-fixture.png"), "fixture export failed");
        }
        integratedUIJourney();
        window.findChild<QAction *>("view.actualPixels")->trigger(); pump();
        auto zoomText=[&] { return window.findChild<QWidget *>("swiftUIStatusBarContainer")->findChild<QLabel *>("zoomStatus")->text(); };
        require(zoomText()=="100.0%","Actual Pixels status does not match viewport");
        window.findChild<QAction *>("view.zoomIn")->trigger(); pump();
        require(zoomText()=="125.0%","Zoom In readout does not match the actual viewport");
        window.findChild<QAction *>("view.fitCanvas")->trigger(); pump();
        // A Wayland compositor may allocate so little space that Fit is exactly 1:1.
        if (qEnvironmentVariable("QT_QPA_PLATFORM")=="offscreen")
            require(zoomText()!="100.0%","Fit still reports the unused shared viewport zoom");
        const QString wideZoom=zoomText();
        // A small laptop/tiled window must retain access to every tool option.
        window.resize(800, 520); window.setTool(SessionWindow::Tool::Brush); pump();
        if (qEnvironmentVariable("QT_QPA_PLATFORM")=="offscreen")
            require(zoomText()!=wideZoom,"resizing did not update fit zoom readout");
        auto *optionsScroll = window.findChild<QScrollArea *>("options.scroll");
        require(optionsScroll && optionsScroll->isVisible(), "small window lost tool options to toolbar overflow");
        auto *lastOption = header()->findChildren<QPushButton *>().value(0);
        require(lastOption, "brush color control missing at small size");
        optionsScroll->ensureWidgetVisible(lastOption); pump();
        require(!lastOption->visibleRegion().isEmpty(), "last tool option cannot be reached by scrolling");
        auto *rail = window.findChild<QWidget *>("swiftUIToolRailContainer")->findChild<QScrollArea *>();
        require(rail, "scrollable tool rail missing");
        rail->verticalScrollBar()->setValue(rail->verticalScrollBar()->maximum()); pump();
        const auto toolButtons = rail->findChildren<QPushButton *>();
        require(!toolButtons.isEmpty() && !toolButtons.last()->visibleRegion().isEmpty(), "last tool cannot be reached in small window");
        if (!output.isEmpty()) require(window.grab().save(output + "/small-window.png"), "small window screenshot failed");
        qInfo("Qt visible UI journey OK (typing/focus, picker, slider, pointer pixels, undo, layer/text shortcuts, window teardown)");
        return 0;
    } catch (const std::exception &e) {
        qCritical("Qt visible UI journey failed: %s", e.what()); return 1;
    }
}
