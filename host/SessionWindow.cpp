#include <QScopeGuard>
#include <QScrollArea>
#include <QLockFile>
#include <QUuid>
#include <QFormLayout>
#include <QDialogButtonBox>
#include <QCloseEvent>
#include <cmath>
#include <QSignalBlocker>
#include "LayerDragModel.h"
#include "ShortcutEditor.h"
#include <QNativeGestureEvent>
// SessionWindow — see SessionWindow.h. Drives the Swift core via compositor_session_*
// and paints the composited RGBA via QImage. Implements file operations using Qt
// codecs (IO milestone: file-map "IO / codec mapping" tier).

#include "SessionWindow.h"
#include "ImageExporters.h"
#include "TabletHandler.h"
#include "LayerItemDelegate.h"
#include "QtPlatformServices.h"
#include "EditorDialogs.h"
#include "SwiftUIQtRenderer.h"

#include <QPainter>
#include <QPainterPath>
#include <QPaintEvent>
#include <QMouseEvent>
#include <QTabletEvent>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QMimeData>
#include <QClipboard>
#include <QGuiApplication>
#include <QUrl>
#include <QMenuBar>
#include <QMessageBox>
#include <QComboBox>
#include <QCheckBox>
#include <QColorDialog>
#include <QPushButton>
#include <QStatusBar>
#include <QDockWidget>
#include <QTreeView>
#include <QStandardItemModel>
#include <QHeaderView>
#include <QStyle>
#include <QHBoxLayout>
#include <QMap>
#include <QSlider>
#include <QVBoxLayout>
#include <QLabel>
#include <QFile>
#include <QImageWriter>
#include <QImageReader>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QFileInfo>
#include <QDir>
#include <QToolBar>
#include <QActionGroup>
#include <QTimer>
#include <QDateTime>
#include <QCloseEvent>
#include <QStackedWidget>
#include <QTabBar>
#include <QSpinBox>
#include <QDoubleSpinBox>
#include <QLineEdit>
#include <QTextEdit>
#include <QButtonGroup>
#include <QFrame>
#include <QToolButton>
#include <QMenu>
#include <QPixmap>
#include <QWheelEvent>
#include <QKeyEvent>
#include <QInputDialog>

#include <cstdint>
#include <cstring>
#include <algorithm>
#include <vector>

// compositor_session_* C ABI (see include/CompositorCore.h). Declared locally so
// the Qt host need not add an include path to the Swift core's headers.
extern "C" {
uint64_t compositor_session_create(void);
void compositor_session_close(uint64_t handle);
int32_t compositor_session_command(uint64_t handle, const uint8_t *json, size_t count);
int64_t compositor_session_render(uint64_t handle, uint8_t *output, size_t capacity);
int64_t compositor_session_state(uint64_t handle, uint8_t *output, size_t capacity);
int64_t compositor_session_export_manifest(uint64_t handle, uint8_t *output, size_t capacity);
int32_t compositor_session_import_manifest(uint64_t handle, const uint8_t *json, size_t count);
int64_t compositor_session_export_layer(uint64_t handle, const uint8_t *layer_id, size_t layer_id_count,
                                        int32_t mask, uint8_t *output, size_t capacity,
                                        size_t *width, size_t *height);
int32_t compositor_session_import_layer(uint64_t handle, const uint8_t *layer_id, size_t layer_id_count,
                                        int32_t mask, const uint8_t *pixels, size_t count,
                                        size_t width, size_t height);
int32_t compositor_session_import_rgba(uint64_t handle, const uint8_t *pixels, size_t count,
                                       size_t width, size_t height, const uint8_t *name, size_t name_count,
                                       int32_t replacing);
}

static int32_t cmd(uint64_t h, const char *json) {
    return compositor_session_command(h, reinterpret_cast<const uint8_t *>(json),
                                      std::strlen(json));
}

static QImage straightRGBA(const std::vector<uint8_t> &premultiplied, int width, int height) {
    std::vector<uint8_t> straight = premultiplied;
    for (size_t i = 0; i + 3 < straight.size(); i += 4) {
        const uint8_t alpha = straight[i + 3];
        if (alpha == 0) {
            straight[i] = straight[i + 1] = straight[i + 2] = 0;
        } else if (alpha != 255) {
            straight[i] = static_cast<uint8_t>(std::min(255, (static_cast<int>(straight[i]) * 255 + alpha / 2) / alpha));
            straight[i + 1] = static_cast<uint8_t>(std::min(255, (static_cast<int>(straight[i + 1]) * 255 + alpha / 2) / alpha));
            straight[i + 2] = static_cast<uint8_t>(std::min(255, (static_cast<int>(straight[i + 2]) * 255 + alpha / 2) / alpha));
        }
    }
    return QImage(reinterpret_cast<const uchar *>(straight.data()), width, height, width * 4,
                  QImage::Format_RGBA8888).copy();
}

static QImage renderToQImage(uint64_t h, int width, int height) {
    const int bytes = width * height * 4;
    std::vector<uint8_t> rgba(static_cast<size_t>(bytes));
    int64_t n = compositor_session_render(h, rgba.data(), rgba.size());
    if (n == bytes) {
        return straightRGBA(rgba, width, height);
    }
    return QImage();
}

class SessionCanvasWidget : public QWidget {
public:
    explicit SessionCanvasWidget(SessionWindow *window) : QWidget(window), m_window(window) {
        setObjectName("editor.canvas");
        setFocusPolicy(Qt::StrongFocus);
        setMouseTracking(true);
        setAcceptDrops(true);
        setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    }
protected:
    void paintEvent(QPaintEvent *event) override {
        m_window->canvasPaintEvent(event, this);
    }
    void resizeEvent(QResizeEvent *event) override {
        QWidget::resizeEvent(event);
        QMetaObject::invokeMethod(m_window, [window=m_window] { window->updateStatusTelemetry(); }, Qt::QueuedConnection);
    }
    void mousePressEvent(QMouseEvent *event) override {
        m_window->canvasMousePressEvent(event, this);
    }
    void mouseMoveEvent(QMouseEvent *event) override {
        m_window->canvasMouseMoveEvent(event, this);
    }
    void mouseReleaseEvent(QMouseEvent *event) override {
        m_window->canvasMouseReleaseEvent(event, this);
    }
    void tabletEvent(QTabletEvent *event) override {
        m_window->canvasTabletEvent(event, this);
    }
    void dragEnterEvent(QDragEnterEvent *event) override {
        m_window->canvasDragEnterEvent(event, this);
    }
    void dropEvent(QDropEvent *event) override {
        m_window->canvasDropEvent(event, this);
    }
    bool event(QEvent *event) override {
        if(event->type()==QEvent::KeyPress && static_cast<QKeyEvent *>(event)->key()==Qt::Key_Tab) {
            m_window->canvasKeyPressEvent(static_cast<QKeyEvent *>(event)); return event->isAccepted();
        }
        if(event->type()==QEvent::NativeGesture) {
            auto *gesture=static_cast<QNativeGestureEvent *>(event);
            if(gesture->gestureType()==Qt::ZoomNativeGesture) {m_window->zoomBy(std::exp(gesture->value())); event->accept(); return true;}
        }
        return QWidget::event(event);
    }
    void wheelEvent(QWheelEvent *event) override {
        if (event->modifiers() & Qt::ControlModifier) {
            if (event->angleDelta().y() > 0) {
                m_window->zoomBy(1.25);
            } else if (event->angleDelta().y() < 0) {
                m_window->zoomBy(1.0 / 1.25);
            }
            event->accept();
        } else {
            const QPoint delta=event->pixelDelta().isNull() ? event->angleDelta()/3 : event->pixelDelta();
            m_window->panCanvas(delta); event->accept();
        }
    }
private:
    SessionWindow *m_window;
};

static QIcon makeToolIcon(SessionWindow::Tool tool,
                          SessionWindow::MarqueeMode marqueeMode = SessionWindow::MarqueeMode::Rectangle,
                          SessionWindow::BrushToolMode brushMode = SessionWindow::BrushToolMode::Paint) {
    QPixmap pix(18, 18);
    pix.fill(Qt::transparent);
    QPainter p(&pix);
    p.setRenderHint(QPainter::Antialiasing, true);
    p.setPen(QPen(QColor(0xd0, 0xd0, 0xd5), 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));

    switch (tool) {
    case SessionWindow::Tool::Move:
        p.drawLine(9, 2, 9, 16);
        p.drawLine(2, 9, 16, 9);
        p.drawLine(9, 2, 6, 5);
        p.drawLine(9, 2, 12, 5);
        p.drawLine(9, 16, 6, 13);
        p.drawLine(9, 16, 12, 13);
        p.drawLine(2, 9, 5, 6);
        p.drawLine(2, 9, 5, 12);
        p.drawLine(16, 9, 13, 6);
        p.drawLine(16, 9, 13, 12);
        break;
    case SessionWindow::Tool::Marquee:
        p.setPen(QPen(QColor(0xd0, 0xd0, 0xd5), 1.5, Qt::DashLine));
        if (marqueeMode == SessionWindow::MarqueeMode::Rectangle) {
            p.drawRect(2, 2, 14, 14);
        } else {
            p.drawEllipse(2, 2, 14, 14);
        }
        break;
    case SessionWindow::Tool::Lasso: {
        QPainterPath path;
        path.moveTo(4, 7);
        path.cubicTo(4, 2, 15, 2, 15, 8);
        path.cubicTo(15, 14, 10, 15, 7, 13);
        path.lineTo(4, 16);
        p.drawPath(path);
        break;
    }
    case SessionWindow::Tool::Magic:
        p.drawLine(3, 15, 13, 5);
        p.drawLine(15, 2, 15, 5);
        p.drawLine(13, 3, 16, 3);
        p.drawLine(11, 2, 11, 3);
        p.drawLine(16, 7, 16, 8);
        break;
    case SessionWindow::Tool::Crop:
        p.drawLine(2, 5, 13, 5);
        p.drawLine(5, 2, 5, 13);
        p.drawLine(5, 13, 16, 13);
        p.drawLine(13, 5, 13, 16);
        break;
    case SessionWindow::Tool::Brush:
        if (brushMode == SessionWindow::BrushToolMode::Paint) {
            p.drawLine(13, 3, 16, 6);
            p.drawLine(13, 3, 9, 8);
            p.drawLine(16, 6, 11, 11);
            p.drawLine(9, 8, 6, 13);
            p.drawLine(11, 11, 6, 13);
            p.setBrush(QColor(0xd0, 0xd0, 0xd5));
            p.drawEllipse(3, 12, 4, 4);
        } else {
            p.drawRoundedRect(3, 5, 12, 8, 2, 2);
            p.drawLine(7, 5, 7, 13);
        }
        break;
    case SessionWindow::Tool::SpotHealing:
        p.save();
        p.translate(9, 9);
        p.rotate(45);
        p.drawRoundedRect(-3, -7, 6, 14, 2, 2);
        p.drawPoint(-1, 0); p.drawPoint(1, 0);
        p.restore();
        break;
    case SessionWindow::Tool::CloneStamp:
        p.drawEllipse(7, 2, 4, 4);
        p.drawLine(9, 6, 9, 10);
        p.drawRoundedRect(4, 10, 10, 4, 1, 1);
        p.fillRect(3, 14, 12, 2, QColor(0xd0, 0xd0, 0xd5));
        break;
    case SessionWindow::Tool::Smear: {
        QPainterPath path;
        path.moveTo(9, 3);
        path.cubicTo(9, 3, 4, 10, 4, 13);
        path.arcTo(4, 8, 10, 10, 180, 180);
        path.cubicTo(14, 10, 9, 3, 9, 3);
        p.drawPath(path);
        break;
    }
    case SessionWindow::Tool::Gradient:
        p.drawRect(2, 2, 14, 14);
        p.fillRect(2, 9, 14, 7, QColor(0xd0, 0xd0, 0xd5));
        break;
    case SessionWindow::Tool::Shape:
        p.drawRect(2, 2, 8, 8);
        p.drawEllipse(7, 7, 9, 9);
        break;
    case SessionWindow::Tool::Type:
        p.drawLine(3, 3, 15, 3);
        p.drawLine(9, 3, 9, 15);
        p.drawLine(6, 15, 12, 15);
        break;
    case SessionWindow::Tool::Eyedropper:
        p.drawLine(4, 14, 6, 12);
        p.drawLine(6, 12, 12, 6);
        p.drawLine(12, 6, 14, 8);
        p.drawLine(14, 8, 8, 14);
        p.drawLine(8, 14, 4, 14);
        p.drawLine(12, 6, 15, 3);
        break;
    case SessionWindow::Tool::Hand:
        p.drawRoundedRect(5, 7, 8, 9, 2, 2);
        p.drawLine(7, 3, 7, 7);
        p.drawLine(9, 2, 9, 7);
        p.drawLine(11, 3, 11, 7);
        break;
    case SessionWindow::Tool::Zoom:
        p.drawEllipse(3, 3, 9, 9);
        p.drawLine(10, 10, 15, 15);
        break;
    case SessionWindow::Tool::Idle:
        break;
    }
    return QIcon(pix);
}

void SessionWindow::initDemoDocument() {
    if (m_sessionHandle == 0) m_sessionHandle = compositor_session_create();
    cmd(m_sessionHandle, R"({"version":1,"action":"new","width":64,"height":64})");
    // "new" already creates a blank "Layer 1" (Sources/LinuxBridge/UpstreamEditor.swift passes
    // emptyLayer: true) — an explicit addLayer here used to be needed but now just adds a duplicate.
    cmd(m_sessionHandle, R"({"version":1,"action":"brushBegin","x":8,"y":8,"parameters":{"diameter":16,"hardness":1,"opacity":1,"red":1,"green":0,"blue":0,"erasing":0}})");
    cmd(m_sessionHandle, R"({"version":1,"action":"brushMove","x":48,"y":48})");
    cmd(m_sessionHandle, R"({"version":1,"action":"brushEnd"})");
    m_image = renderToQImage(m_sessionHandle, 64, 64);
    m_hasDocument = true;
    refreshLayers();
}

void SessionWindow::createNewDocument(int width, int height) {
    if (m_sessionHandle == 0) m_sessionHandle = compositor_session_create();
    const QString json = QString(R"({"version":1,"action":"new","width":%1,"height":%2})").arg(width).arg(height);
    const QByteArray bytes = json.toUtf8();
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) != 0) return;
    m_projectPath.clear();
    m_recoveredUnsaved = false;
    // "new" already creates a blank "Layer 1" (emptyLayer: true) — no explicit addLayer needed here.
    m_image = renderToQImage(m_sessionHandle, width, height);
    m_hasDocument = true;
    m_zoomLevel = 0.0;
    m_panOffset = QPointF(0, 0);
    refreshImage();
    refreshLayers();
    if (m_documentTabBar && m_documentTabBar->count() > 0) {
        m_documentTabBar->setTabText(0, tr("Untitled 1"));
    }
}

SessionWindow::SessionWindow(QWidget *parent, PlatformServices services)
    : QMainWindow(parent), m_platform(services.withDefaults()) {
    m_recoveryID = QUuid::createUuid().toString(QUuid::WithoutBraces);
    setWindowTitle("Compositor");
    resize(ParityMetrics::WindowDefaultWidth, ParityMetrics::WindowDefaultHeight);
    setMinimumSize(ParityMetrics::WindowMinWidth, ParityMetrics::WindowMinHeight);
    setAcceptDrops(true);
    m_tabletHandler = std::make_unique<PressureModulatedTabletHandler>();
    applyDarkTheme();

    // Central canvas widget:
    m_canvasWidget = new SessionCanvasWidget(this);
    setCentralWidget(m_canvasWidget);

    m_toolsBar = addToolBar(tr("Tools"));
    m_toolsBar->setObjectName("toolbar.tools");
    m_toolsBar->setMovable(false);
    m_toolsBar->setOrientation(Qt::Vertical);
    m_toolsBar->setToolButtonStyle(Qt::ToolButtonIconOnly);
    m_toolsBar->setIconSize(QSize(ParityMetrics::ToolIconNominal, ParityMetrics::ToolIconNominal));
    addToolBar(Qt::LeftToolBarArea, m_toolsBar);
    auto *toolGroup = new QActionGroup(this);
    toolGroup->setExclusive(true);

    auto addToolAct = [&](const QString &title, QKeySequence shortcut, Tool t, const char *objName) {
        auto *act = new QAction(makeToolIcon(t), title, this);
        act->setShortcut(shortcut);
        QString cleanTitle = title;
        cleanTitle.remove('&');
        act->setToolTip(QString("%1 (%2)").arg(cleanTitle).arg(shortcut.toString(QKeySequence::NativeText)));
        act->setObjectName(objName);
        act->setCheckable(true);
        connect(act, &QAction::triggered, this, [this, t] { setTool(t); });
        toolGroup->addAction(act);
        m_toolsBar->addAction(act);
        m_toolActions[t] = act;
        return act;
    };

    auto *actMove = addToolAct(tr("Move / Transform (V)"), QKeySequence(Qt::Key_V), Tool::Move, "tool.move");
    addToolAct(tr("Marquee (M)"), QKeySequence(Qt::Key_M), Tool::Marquee, "tool.marquee");
    addToolAct(tr("Lasso (L)"), QKeySequence(Qt::Key_L), Tool::Lasso, "tool.lasso");
    addToolAct(tr("Magic (W)"), QKeySequence(Qt::Key_W), Tool::Magic, "tool.magic");
    addToolAct(tr("Crop (C)"), QKeySequence(Qt::Key_C), Tool::Crop, "tool.crop");
    addToolAct(tr("Brush (B) · Eraser (E)"), QKeySequence(Qt::Key_B), Tool::Brush, "tool.brush");
    addToolAct(tr("Spot Healing Brush (J)"), QKeySequence(Qt::Key_J), Tool::SpotHealing, "tool.spotHealing");
    addToolAct(tr("Clone Stamp (S)"), QKeySequence(Qt::Key_S), Tool::CloneStamp, "tool.cloneStamp");
    addToolAct(tr("Smear (R)"), QKeySequence(Qt::Key_R), Tool::Smear, "tool.blur");
    addToolAct(tr("Gradient (G)"), QKeySequence(Qt::Key_G), Tool::Gradient, "tool.gradient");
    addToolAct(tr("Shape (U)"), QKeySequence(Qt::Key_U), Tool::Shape, "tool.shape");
    addToolAct(tr("Type (T)"), QKeySequence(Qt::Key_T), Tool::Type, "tool.type");
    addToolAct(tr("Eyedropper (I)"), QKeySequence(Qt::Key_I), Tool::Eyedropper, "tool.eyedropper");
    addToolAct(tr("Hand (H)"), QKeySequence(Qt::Key_H), Tool::Hand, "tool.hand");
    addToolAct(tr("Zoom (Z)"), QKeySequence(Qt::Key_Z), Tool::Zoom, "tool.zoom");

    // Hidden action aliases for compatibility with automated smoke tests looking for older names
    auto addHiddenAlias = [&](const char *name, std::function<void()> slot) {
        auto *act = new QAction(this);
        act->setObjectName(name);
        connect(act, &QAction::triggered, this, slot);
        addAction(act);
    };
    addHiddenAlias("tool.rectSelect", [this] { setMarqueeMode(MarqueeMode::Rectangle); setTool(Tool::Marquee); });
    addHiddenAlias("tool.ellipseSelect", [this] { setMarqueeMode(MarqueeMode::Ellipse); setTool(Tool::Marquee); });
    addHiddenAlias("tool.magicWand", [this] { setMagicMode(MagicMode::Wand); setTool(Tool::Magic); });
    addHiddenAlias("tool.eraser", [this] { setBrushToolMode(BrushToolMode::Erase); setTool(Tool::Brush); });

    auto *paletteWidget = new QWidget(m_toolsBar);
    paletteWidget->setObjectName("palette.controls");
    paletteWidget->setFixedSize(ParityMetrics::PaletteFrameWidth, ParityMetrics::PaletteFrameHeight);

    m_bgColorButton = new QPushButton(paletteWidget);
    m_bgColorButton->setObjectName("palette.background");
    m_bgColorButton->setGeometry(12, 12, ParityMetrics::SwatchSize, ParityMetrics::SwatchSize);
    m_bgColorButton->setStyleSheet(QString("background-color: %1; border: 1.5px solid #ffffff; border-radius: %2px;").arg(m_backgroundColor.name()).arg(ParityMetrics::SwatchCornerRadius));
    m_bgColorButton->setToolTip(tr("Background color (click to change)"));
    connect(m_bgColorButton, &QPushButton::clicked, this, &SessionWindow::pickBackgroundColor);

    m_brushColorButton = new QPushButton(paletteWidget);
    m_brushColorButton->setObjectName("brush.color");
    m_brushColorButton->setGeometry(0, 0, ParityMetrics::SwatchSize, ParityMetrics::SwatchSize);
    m_brushColorButton->setStyleSheet(QString("background-color: %1; border: 1.5px solid #ffffff; border-radius: %2px;").arg(m_brushColor.name()).arg(ParityMetrics::SwatchCornerRadius));
    m_brushColorButton->setToolTip(tr("Foreground color (click to change)"));
    connect(m_brushColorButton, &QPushButton::clicked, this, &SessionWindow::pickBrushColor);

    auto *btnSwap = new QToolButton(paletteWidget);
    btnSwap->setGeometry(27, -3, ParityMetrics::SwapIconSize, ParityMetrics::SwapIconSize);
    btnSwap->setAutoRaise(true);
    btnSwap->setText(QString::fromUtf8("⇄"));
    btnSwap->setStyleSheet("color: #f5f5f7; font-size: 10px; font-weight: bold; border: none; padding: 0px;");
    btnSwap->setToolTip(tr("Swap colors (X)"));
    connect(btnSwap, &QToolButton::clicked, this, &SessionWindow::swapPaletteColors);

    auto *btnReset = new QToolButton(paletteWidget);
    btnReset->setGeometry(-1, 27, ParityMetrics::ResetIconSize, ParityMetrics::ResetIconSize);
    btnReset->setAutoRaise(true);
    btnReset->setText(QString::fromUtf8("⟲"));
    btnReset->setStyleSheet("color: #f5f5f7; font-size: 9px; font-weight: bold; border: none; padding: 0px;");
    btnReset->setToolTip(tr("Default black/white (D)"));
    connect(btnReset, &QToolButton::clicked, this, &SessionWindow::resetPaletteColors);

    m_toolsBar->addWidget(paletteWidget);

    m_sessionHandle = compositor_session_create();
    const bool isSmokeTest = qApp && (
        qApp->arguments().contains("--ui-smoke") ||
        qApp->arguments().contains("--dialog-smoke") ||
        qApp->arguments().contains("--brush-smoke") ||
        qApp->arguments().contains("--layers-smoke") ||
        qApp->arguments().contains("--io-smoke") ||
        qEnvironmentVariableIsSet("COMPOSITOR_DEMO_CANVAS")
    );
    if (isSmokeTest) {
        initDemoDocument();
        setBrushColor(QColor(255, 0, 0));
    } else {
        createNewDocument(1024, 768);
        setBrushColor(QColor(0, 0, 0));
    }

    actMove->setChecked(true);
    setTool(Tool::Move);

    m_layersDock = new QDockWidget(tr("Layers"), this);
    auto *dock = m_layersDock;
    dock->setObjectName("dock.layers");
    auto *panel = new QWidget(dock);
    auto *layout = new QVBoxLayout(panel);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(6);

    // Header: "Layers" and layer count
    auto *headerLayout = new QHBoxLayout();
    auto *lblLayersTitle = new QLabel(tr("Layers"), panel);
    lblLayersTitle->setStyleSheet("font-size: 12px; font-weight: 600; color: #ffffff;");
    m_layerCountLabel = new QLabel(tr("1"), panel);
    m_layerCountLabel->setObjectName("layerCount");
    m_layerCountLabel->setStyleSheet("font-size: 11px; color: #707074; font-family: monospace;");
    headerLayout->addWidget(lblLayersTitle);
    headerLayout->addStretch();
    headerLayout->addWidget(m_layerCountLabel);
    layout->addLayout(headerLayout);

    // Appearance Controls: Blend & Opacity
    auto *blendLayout = new QHBoxLayout();
    auto *lblBlend = new QLabel(tr("Blend"), panel);
    lblBlend->setStyleSheet("font-size: 11px; color: #a0a0a5;");
    blendLayout->addWidget(lblBlend);
    m_blend = new QComboBox(panel);
    m_blend->setObjectName("blend.mode");
    for (const QString &mode : blendModes()) m_blend->addItem(mode);
    blendLayout->addWidget(m_blend, 1);
    layout->addLayout(blendLayout);

    auto *opacityLayout = new QHBoxLayout();
    auto *lblOpacity = new QLabel(tr("Opacity"), panel);
    lblOpacity->setStyleSheet("font-size: 11px; color: #a0a0a5;");
    opacityLayout->addWidget(lblOpacity);
    m_opacity = new QSlider(Qt::Horizontal, panel);
    m_opacity->setObjectName("layer.opacity");
    m_opacity->setRange(0, 100);
    m_opacity->setValue(100);
    opacityLayout->addWidget(m_opacity, 1);
    m_opacityLabel = new QLabel(tr("100 %"), panel);
    m_opacityLabel->setFixedWidth(40);
    m_opacityLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    m_opacityLabel->setStyleSheet("font-size: 11px; color: #d0d0d5;");
    opacityLayout->addWidget(m_opacityLabel);
    layout->addLayout(opacityLayout);

    m_layersView = new LayerTreeView(panel);
    m_layersView->installEventFilter(this);
    m_layersView->setDragEnabled(true);
    m_layersView->setAcceptDrops(true);
    m_layersView->setDropIndicatorShown(true);
    m_layersView->setDragDropMode(QAbstractItemView::DragDrop);
    m_layersView->setDefaultDropAction(Qt::MoveAction);
    m_layersView->setObjectName("layers.treeView");
    m_layerModel = new LayerDragModel(this);
    m_layerModel->setHorizontalHeaderLabels({tr("Layer"), tr("Visible"), tr("Mask")});
    m_layersView->setModel(m_layerModel);
    static_cast<LayerDragModel *>(m_layerModel)->place = [this](QJsonObject command, bool check) {
        command.insert("version",1);
        const auto bytes=QJsonDocument(command).toJson(QJsonDocument::Compact);
        const bool ok=compositor_session_command(m_sessionHandle,reinterpret_cast<const uint8_t *>(bytes.constData()),bytes.size())==0;
        if(ok && !check) QTimer::singleShot(0,this,[this] {refreshImage(); refreshLayers();});
        return ok;
    };
    m_layersView->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_layersView,&QWidget::customContextMenuRequested,this,[this](QPoint pos) {
        auto index=m_layersView->indexAt(pos);
        if(index.isValid() && !m_layersView->selectionModel()->isSelected(index)) m_layersView->setCurrentIndex(index);
        refreshMenuTitles(sessionState());
        QMenu menu(this); menu.setObjectName("layers.contextMenu");
        for(const char *name:{"layer.rename","layer.duplicate","layer.addGroup","layer.moveOutOfFolder","layer.merge",
                              "layer.showHide","layer.addRevealMask","layer.deleteMask","layer.delete"})
            if(auto *action=findChild<QAction *>(name)) menu.addAction(action);
        menu.exec(m_layersView->viewport()->mapToGlobal(pos));
    });

    m_layersView->setSelectionMode(QAbstractItemView::ExtendedSelection);
    m_layersView->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_layersView->setUniformRowHeights(true);
    m_layersView->setAnimated(false);
    m_layersView->setAllColumnsShowFocus(true);
    m_layersView->setRootIsDecorated(false);
    m_layersView->setIndentation(18);
    m_layersView->setHeaderHidden(true);
    m_layersView->setMouseTracking(true);
    m_layersView->setFrameShape(QFrame::NoFrame);
    m_layersView->setItemDelegateForColumn(0, new LayerItemDelegate(m_layersView));
    m_layersView->header()->setStretchLastSection(false);
    m_layersView->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    // Visibility / mask state stays in the model (columns 1-2) but is drawn by
    // the row delegate (eye icon, mask badge) instead of as separate columns.
    m_layersView->setColumnHidden(1, true);
    m_layersView->setColumnHidden(2, true);
    layout->addWidget(m_layersView, 1);

    auto *checksLayout = new QHBoxLayout();
    m_visibleCheck = new QCheckBox(tr("Visible"), panel);
    m_visibleCheck->setObjectName("layer.visible");
    m_visibleCheck->setVisible(false);  // superseded by the eye icon in each row; kept for automation
    m_maskCheck = new QCheckBox(tr("Mask enabled"), panel);
    m_maskCheck->setObjectName("layer.mask");
    checksLayout->addWidget(m_visibleCheck);
    checksLayout->addWidget(m_maskCheck);
    checksLayout->addStretch();
    layout->addLayout(checksLayout);

    // Bottom icon bar: add layer, folder, mask, adjustment, delete.
    auto glyph = [](int kind) {
        QPixmap pm(40, 40);
        pm.fill(Qt::transparent);
        QPainter g(&pm);
        g.setRenderHint(QPainter::Antialiasing, true);
        g.setPen(QPen(QColor(0xb4, 0xb4, 0xba), 2.0, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        g.setBrush(Qt::NoBrush);
        switch (kind) {
        case 0:  // add layer
            g.drawRoundedRect(QRectF(8, 8, 24, 24), 4, 4);
            g.drawLine(20, 14, 20, 26); g.drawLine(14, 20, 26, 20);
            break;
        case 1:  // folder
            g.drawRoundedRect(QRectF(7, 13, 26, 18), 3, 3);
            g.drawLine(8, 13, 8, 10); g.drawLine(8, 10, 16, 10); g.drawLine(16, 10, 19, 13);
            break;
        case 2:  // mask
            g.drawRoundedRect(QRectF(8, 8, 24, 24), 4, 4);
            g.setBrush(QColor(0xb4, 0xb4, 0xba));
            g.drawEllipse(QPointF(20, 20), 6, 6);
            break;
        case 3:  // adjustment
            g.drawEllipse(QPointF(20, 20), 12, 12);
            g.setBrush(QColor(0xb4, 0xb4, 0xba));
            { QPainterPath half; half.moveTo(20, 8); half.arcTo(QRectF(8, 8, 24, 24), 90, -180); half.closeSubpath(); g.drawPath(half); }
            break;
        default:  // trash
            g.drawLine(11, 12, 29, 12); g.drawLine(16, 12, 16, 9); g.drawLine(16, 9, 24, 9); g.drawLine(24, 9, 24, 12);
            g.drawRoundedRect(QRectF(13, 12, 14, 19), 2, 2);
            g.drawLine(18, 17, 18, 26); g.drawLine(22, 17, 22, 26);
            break;
        }
        pm.setDevicePixelRatio(2.0);
        return QIcon(pm.scaled(40, 40));
    };
    auto makeIconButton = [&](int kind, const QString &name, const QString &tip) {
        auto *button = new QToolButton(panel);
        button->setObjectName(name);
        button->setToolTip(tip);
        button->setAccessibleName(tip);
        button->setIcon(glyph(kind));
        button->setIconSize(QSize(20, 20));
        button->setAutoRaise(true);
        button->setFixedSize(30, 28);
        button->setCursor(Qt::PointingHandCursor);
        return button;
    };
    auto *btnLayout = new QHBoxLayout();
    btnLayout->setContentsMargins(0, 4, 0, 0);
    btnLayout->setSpacing(6);
    auto *btnAddLayer = makeIconButton(0, "layer.add", tr("Add blank layer"));
    auto *btnAddGroup = makeIconButton(1, "layer.addGroup", tr("Add new group/folder"));
    auto *btnAddMask = makeIconButton(2, "layer.addMask", tr("Add reveal layer mask"));
    auto *btnAdjustment = makeIconButton(3, "layer.addAdjustment", tr("Add adjustment layer"));
    auto *btnDelete = makeIconButton(4, "layer.delete", tr("Delete active layer or group"));
    auto *adjustmentMenu = new QMenu(btnAdjustment);
    for (const QString &kind : {QString("Hue/Saturation"), QString("Levels"), QString("Curves"),
                                QString("Exposure"), QString("Grain"), QString("Gradient Map")}) {
        adjustmentMenu->addAction(kind, this, [this, kind] { showAdjustDialog(kind); });
    }
    btnAdjustment->setMenu(adjustmentMenu);
    btnAdjustment->setPopupMode(QToolButton::InstantPopup);
    btnLayout->addWidget(btnAddLayer);
    btnLayout->addWidget(btnAddGroup);
    btnLayout->addWidget(btnAddMask);
    btnLayout->addWidget(btnAdjustment);
    btnLayout->addStretch();
    btnLayout->addWidget(btnDelete);
    layout->addLayout(btnLayout);

    connect(btnAddLayer, &QToolButton::clicked, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"addLayer"})") == 0) { refreshImage(); refreshLayers(); }
    });
    connect(btnAddGroup, &QToolButton::clicked, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"addGroup"})") == 0) { refreshImage(); refreshLayers(); }
    });
    connect(btnAddMask, &QToolButton::clicked, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"addRevealMask"})") == 0) { refreshImage(); refreshLayers(); }
    });
    connect(btnDelete, &QToolButton::clicked, this, [this] {
        deleteSelectedLayers();
    });

    dock->setMinimumWidth(252);
    dock->setMaximumWidth(352);
    panel->setMinimumWidth(252);
    panel->setMaximumWidth(352);
    dock->setWidget(panel);
    addDockWidget(Qt::RightDockWidgetArea, dock);

    // Floating "Adjustments" palette: one row per adjustment, opens its sheet.
    {
        m_adjustmentsDock = new QDockWidget(tr("Adjustments"), this);
        auto *adjDock = m_adjustmentsDock;
        adjDock->setObjectName("dock.adjustments");
        adjDock->setFeatures(QDockWidget::DockWidgetClosable | QDockWidget::DockWidgetFloatable | QDockWidget::DockWidgetMovable);
        auto *body = new QWidget(adjDock);
        auto *rows = new QVBoxLayout(body);
        rows->setContentsMargins(6, 6, 6, 8);
        rows->setSpacing(2);
        auto adjustmentIcon = [](int kind) {
            QPixmap pm(48, 48);
            pm.fill(Qt::transparent);
            QPainter g(&pm);
            g.setRenderHint(QPainter::Antialiasing, true);
            const QColor ink(0xc8, 0xc8, 0xce);
            g.setPen(QPen(ink, 2.2, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
            g.setBrush(Qt::NoBrush);
            switch (kind) {
            case 0:  // Hue/Saturation: half-filled disc
                g.drawEllipse(QPointF(24, 24), 15, 15);
                g.setBrush(ink);
                { QPainterPath half; half.moveTo(24, 9); half.arcTo(QRectF(9, 9, 30, 30), 90, -180); half.closeSubpath(); g.drawPath(half); }
                break;
            case 1:  // Levels: two sliders
                g.drawLine(8, 17, 40, 17); g.drawLine(8, 31, 40, 31);
                g.setBrush(QColor(0x1e, 0x1e, 0x20)); g.drawEllipse(QPointF(29, 17), 4, 4); g.drawEllipse(QPointF(18, 31), 4, 4);
                break;
            case 2:  // Curves
                { QPainterPath c; c.moveTo(8, 40); c.cubicTo(20, 40, 24, 10, 40, 8); g.drawPath(c); g.setBrush(ink); g.drawEllipse(QPointF(8, 40), 3, 3); g.drawEllipse(QPointF(40, 8), 3, 3); }
                break;
            case 3:  // Exposure: plus/minus in a circle
                g.drawEllipse(QPointF(24, 24), 15, 15);
                g.drawLine(24, 15, 24, 25); g.drawLine(19, 20, 29, 20); g.drawLine(19, 31, 29, 31);
                break;
            case 4:  // Grain: dot lattice
                g.setPen(Qt::NoPen); g.setBrush(ink);
                for (int ix = 0; ix < 4; ++ix) for (int iy = 0; iy < 4; ++iy) g.drawEllipse(QPointF(10 + ix * 9.3, 10 + iy * 9.3), 1.8, 1.8);
                break;
            default:  // Gradient Map: palette blob
                g.drawEllipse(QPointF(24, 24), 15, 15);
                g.setPen(Qt::NoPen); g.setBrush(ink);
                g.drawEllipse(QPointF(18, 19), 2.4, 2.4); g.drawEllipse(QPointF(28, 18), 2.4, 2.4);
                g.drawEllipse(QPointF(16, 28), 2.4, 2.4); g.drawEllipse(QPointF(27, 30), 2.4, 2.4);
                break;
            }
            pm.setDevicePixelRatio(2.0);
            return QIcon(pm);
        };
        const QStringList kinds = {"Hue/Saturation", "Levels", "Curves", "Exposure", "Grain", "Gradient Map"};
        for (int i = 0; i < kinds.size(); ++i) {
            const QString kind = kinds.at(i);
            auto *row = new QToolButton(body);
            row->setObjectName("adjustments." + kind);
            row->setText(kind);
            row->setIcon(adjustmentIcon(i));
            row->setIconSize(QSize(24, 24));
            row->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
            row->setAutoRaise(true);
            row->setCursor(Qt::PointingHandCursor);
            row->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
            row->setMinimumHeight(38);
            row->setStyleSheet("QToolButton { text-align: left; padding: 4px 8px; font-size: 13px; color: #e6e6ea; border: none; border-radius: 6px; }"
                               "QToolButton:hover { background: #2c2d31; }");
            connect(row, &QToolButton::clicked, this, [this, kind] { showAdjustDialog(kind); });
            rows->addWidget(row);
        }
        rows->addStretch();
        adjDock->setWidget(body);
        adjDock->setMinimumWidth(220);
        addDockWidget(Qt::RightDockWidgetArea, adjDock);
        adjDock->setFloating(true);
        adjDock->hide();
        QAction *toggle = adjDock->toggleViewAction();
        toggle->setText(tr("Adjustments Panel"));
        toggle->setObjectName("view.adjustmentsPanel");
    }

    connect(m_layersView->selectionModel(), &QItemSelectionModel::selectionChanged, this, [this] {
        if(m_syncingLayers) return;
        QJsonArray ids; for(auto index:m_layersView->selectionModel()->selectedRows()) ids.append(index.data(Qt::UserRole).toString());
        if(ids.isEmpty()) return;
        sendCommand({{"action","selectLayers"},{"layerIDs",ids},{"layerID",m_layersView->currentIndex().data(Qt::UserRole).toString()}});
        const auto state=sessionState(); refreshMenuTitles(state);
        // Keep the active transform in sync without rebuilding the tree mid-selection.
        const QString active=state.value("activeLayerID").toString();
        m_activeGeometry=LayerGeometry();
        for(const auto &geometry:m_layerGeometries) if(geometry.id==active) m_activeGeometry=geometry;
        syncTransformFields();
        QSignalBlocker opacity(m_opacity);
        for(auto value:state.value("layers").toArray()) {
            auto layer=value.toObject(); if(layer.value("id")==state.value("activeLayerID")) m_opacity->setValue(qRound(layer.value("opacity").toDouble()*100));
        }
        if(m_canvasWidget) m_canvasWidget->update();
    });

    connect(m_layerModel, &QStandardItemModel::itemChanged, this, [this](QStandardItem *item) {
        if (m_syncingLayers || !item || m_sessionHandle == 0) return;
        m_syncingLayers = true;
        if (item->column() == 0) {
            const QString id = item->data(Qt::UserRole).toString();
            const QString newName = item->text();
            if (!id.isEmpty() && !newName.isEmpty()) {
                QJsonObject selectCmd{{"action", "selectLayer"}, {"layerID", id}};
                sendCommand(selectCmd);
                QJsonObject renameCmd{{"action", "renameLayer"}, {"name", newName}};
                if (sendCommand(renameCmd)) refreshImage();
            }
        } else if (item->column() == 1) {
            const QModelIndex nameIndex = m_layerModel->index(item->row(), 0, item->parent() ? item->parent()->index() : QModelIndex());
            const QString id = nameIndex.data(Qt::UserRole).toString();
            const bool visible = (item->checkState() == Qt::Checked);
            if (!id.isEmpty()) {
                QJsonObject selectCmd{{"action", "selectLayer"}, {"layerID", id}};
                sendCommand(selectCmd);
                setLayerFlag("setVisible", visible);
                refreshImage();
            }
        } else if (item->column() == 2) {
            const QModelIndex nameIndex = m_layerModel->index(item->row(), 0, item->parent() ? item->parent()->index() : QModelIndex());
            const QString id = nameIndex.data(Qt::UserRole).toString();
            const bool enabled = (item->checkState() == Qt::Checked);
            if (!id.isEmpty()) {
                QJsonObject selectCmd{{"action", "selectLayer"}, {"layerID", id}};
                sendCommand(selectCmd);
                sendCommand({{"action", "setMaskEnabled"}, {"enabled", enabled}});
                refreshImage();
            }
        }
        m_syncingLayers = false;
    });

    connect(m_opacity, &QSlider::valueChanged, this, &SessionWindow::setOpacityFromSlider);
    connect(m_blend, &QComboBox::currentIndexChanged, this, &SessionWindow::setBlendModeFromCombo);
    connect(m_visibleCheck, &QCheckBox::toggled, this, [this](bool on) { if (setLayerFlag("setVisible", on)) refreshImage(); });
    connect(m_maskCheck, &QCheckBox::toggled, this, [this](bool on) {
        if (sendCommand({{"action", "setMaskEnabled"}, {"enabled", on}})) refreshImage();
    });
    setupHeaderBar();
    setupOptionsBar();
    createMenus();
    refreshLayers();
    updateOptionsBar();
    updateToolRail();
    updateStatusTelemetry();

    updateDocumentTitle();
    applyShortcuts();
    registerSwiftUIActionListener(this, [this](uint64_t handle, const QString &panel) {
        if (handle != m_sessionHandle) return;
        if (panel == "ToolRail") syncToolFromSession();
        syncSharedControls();
        if(panel=="ToolHeaders") sendCommand({{"action","syncToolSettings"}});
        syncSharedControls();
        updateOptionsBar(); refreshImage();
        presentSharedColorPicker();
        if(sessionState().contains("textContent")) showTextEditor();
    });

    m_autosaveTimer = new QTimer(this);
    m_autosaveTimer->setObjectName("autosaveTimer");
    m_autosaveTimer->setInterval(60000);
    connect(m_autosaveTimer, &QTimer::timeout, this, [this] { performAutosave(); });
    m_autosaveTimer->start();
}

SessionWindow::~SessionWindow() {
    if (m_sessionHandle != 0) compositor_session_close(m_sessionHandle);
}

void SessionWindow::setMarqueeMode(MarqueeMode mode) {
    m_marqueeMode = mode;
    if (m_sessionHandle && m_tool == Tool::Marquee)
        sendCommand({{"action", "selectTool"}, {"kind", "marquee"}, {"name", mode == MarqueeMode::Ellipse ? "Ellipse" : "Rectangle"}});
    if (m_toolActions.contains(Tool::Marquee)) {
        m_toolActions[Tool::Marquee]->setIcon(makeToolIcon(Tool::Marquee, m_marqueeMode, m_brushToolMode));
        m_toolActions[Tool::Marquee]->setToolTip(mode == MarqueeMode::Rectangle
            ? tr("Rectangular Marquee (M)") : tr("Elliptical Marquee (M)"));
    }
    updateOptionsBar();
}

void SessionWindow::cycleMarqueeMode() {
    setMarqueeMode(m_marqueeMode == MarqueeMode::Rectangle ? MarqueeMode::Ellipse : MarqueeMode::Rectangle);
}

void SessionWindow::setLassoMode(LassoMode mode) {
    m_lassoMode = mode;
    if (m_sessionHandle && m_tool == Tool::Lasso)
        sendCommand({{"action", "selectTool"}, {"kind", "lasso"}, {"name", mode == LassoMode::Polygonal ? "Polygonal" : "Freehand"}});
    m_polygonalLasso = (mode == LassoMode::Polygonal);
    if (m_toolActions.contains(Tool::Lasso)) {
        m_toolActions[Tool::Lasso]->setToolTip(mode == LassoMode::Freehand
            ? tr("Lasso (L)") : tr("Polygonal Lasso (L)"));
    }
    updateOptionsBar();
}

void SessionWindow::cycleLassoMode() {
    setLassoMode(m_lassoMode == LassoMode::Freehand ? LassoMode::Polygonal : LassoMode::Freehand);
}

void SessionWindow::setMagicMode(MagicMode mode) {
    m_magicMode = mode;
    if (m_toolActions.contains(Tool::Magic)) {
        m_toolActions[Tool::Magic]->setToolTip(mode == MagicMode::Wand
            ? tr("Magic Wand (W)") : tr("Magic Object (W)"));
    }
    updateOptionsBar();
}

void SessionWindow::toggleMagicMode() {
    setMagicMode(m_magicMode == MagicMode::Wand ? MagicMode::Object : MagicMode::Wand);
}

void SessionWindow::setBrushToolMode(BrushToolMode mode) {
    m_brushToolMode = mode;
    sendCommand({{"action","setBrushSettings"},{"parameters",QJsonObject{{"erasing",mode==BrushToolMode::Erase ? 1 : 0}}}});
    m_brushMode = (mode == BrushToolMode::Erase) ? "Erase" : "Paint";
    if (m_toolActions.contains(Tool::Brush)) {
        m_toolActions[Tool::Brush]->setIcon(makeToolIcon(Tool::Brush, m_marqueeMode, m_brushToolMode));
        m_toolActions[Tool::Brush]->setToolTip(mode == BrushToolMode::Erase
            ? tr("Eraser (E)") : tr("Brush (B) · Eraser (E)"));
    }
    updateOptionsBar();
}

void SessionWindow::setSmearMode(SmearMode mode) {
    m_smearMode = mode;
    updateOptionsBar();
}

void SessionWindow::setSpotHealingMode(SpotHealingMode mode) {
    m_spotHealingMode = mode;
    updateOptionsBar();
}

void SessionWindow::setShapeMode(ShapeMode mode) {
    m_shapeMode = mode;
    if(m_tool==Tool::Shape) sendCommand({{"action","selectTool"},{"kind","shape"},{"name",mode==ShapeMode::Ellipse ? "Ellipse" : mode==ShapeMode::Line ? "Line" : "Rectangle"}});
    updateOptionsBar();
}

void SessionWindow::cycleShapeMode() {
    switch (m_shapeMode) {
    case ShapeMode::Rectangle: setShapeMode(ShapeMode::Ellipse); break;
    case ShapeMode::Ellipse: setShapeMode(ShapeMode::Line); break;
    case ShapeMode::Line: setShapeMode(ShapeMode::Rectangle); break;
    }
}

void SessionWindow::applyCrop() {
    if(!m_hasPendingCrop) return;
    sendCommand({{"action","toolApply"}}); syncSharedControls(); refreshImage(); updateOptionsBar();
}
void SessionWindow::cancelCrop() {
    sendCommand({{"action","toolCancel"}}); syncSharedControls(); updateOptionsBar();
    if(m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::swapPaletteColors() {
    sendCommand({{"action","swapPalette"}}); syncSharedControls(); updateOptionsBar();
}
void SessionWindow::resetPaletteColors() {
    sendCommand({{"action","resetPalette"}}); syncSharedControls(); updateOptionsBar();
}

void SessionWindow::setBackgroundColor(const QColor &color) {
    sendCommand({{"action","setPaletteColor"},{"enabled",true},{"parameters",QJsonObject{{"red",color.redF()},{"green",color.greenF()},{"blue",color.blueF()}}}});
    m_backgroundColor = color;
    if (m_bgColorButton) {
        m_bgColorButton->setStyleSheet(QString("background-color: %1; border: 1px solid #000000; border-radius: %2px;")
            .arg(m_backgroundColor.name()).arg(ParityMetrics::SwatchCornerRadius));
    }
}

void SessionWindow::pickBackgroundColor() {
    sendCommand({{"action","openColorPicker"},{"enabled",true}}); presentSharedColorPicker();
}

bool SessionWindow::eventFilter(QObject *watched, QEvent *event) {
    if (watched == m_layersView && event->type() == QEvent::KeyPress) {
        auto *key = static_cast<QKeyEvent *>(event);
        const auto input=shortcuts::chord(QKeySequence(key->keyCombination()));
        const auto typed=shortcuts::sequence(input.value("key").toString(),input.value("modifiers").toInt());
        bool toolKey=false;
        for(auto v:m_shortcuts) {auto d=v.toObject(); if(d.value("group")=="Canvas & Layers" && shortcuts::supported(d))
            toolKey=toolKey || shortcuts::sequence(d.value("key").toString(),d.value("modifiers").toInt())==typed;}
        if (toolKey) {
            key->setAccepted(false);
            keyPressEvent(key);
            return key->isAccepted();
        }
    }
    return QMainWindow::eventFilter(watched, event);
}

void SessionWindow::keyPressEvent(QKeyEvent *event) {
    // Responder check: editable text controls handle their own keys
    QWidget *focus = QApplication::focusWidget();
    if (focus && (qobject_cast<QLineEdit *>(focus) || qobject_cast<QAbstractSpinBox *>(focus) || qobject_cast<QTextEdit *>(focus))) {
        QMainWindow::keyPressEvent(event);
        return;
    }

    if(!m_mappingShortcut) {
        const auto input=shortcuts::chord(QKeySequence(event->keyCombination()));
        const auto typed=shortcuts::sequence(input.value("key").toString(),input.value("modifiers").toInt());
        bool displaced=false;
        for(auto v:m_shortcuts) {
            auto d=v.toObject(); if(d.value("group")!="Canvas & Layers" || !shortcuts::supported(d)) continue;
            const auto current=shortcuts::sequence(d.value("key").toString(),d.value("modifiers").toInt());
            const auto original=shortcuts::sequence(d.value("originalKey").toString(),d.value("originalModifiers").toInt());
            if(current==typed && current!=original) {
                QKeyEvent mapped(event->type(),original[0].key(),original[0].keyboardModifiers(),d.value("originalKey").toString(),event->isAutoRepeat());
                m_mappingShortcut=true; keyPressEvent(&mapped); m_mappingShortcut=false; event->setAccepted(mapped.isAccepted()); return;
            }
            displaced=displaced || (original==typed && current!=original);
        }
        if(displaced) {event->accept(); return;}
    }

    const bool brushTool = m_tool == Tool::Brush || m_tool == Tool::CloneStamp || m_tool == Tool::SpotHealing || m_tool == Tool::Smear;
    const bool leftBracket = event->key() == Qt::Key_BracketLeft || event->key() == Qt::Key_BraceLeft;
    const bool rightBracket = event->key() == Qt::Key_BracketRight || event->key() == Qt::Key_BraceRight;
    if (brushTool && (leftBracket || rightBracket) &&
        (event->modifiers() == Qt::NoModifier || event->modifiers() == Qt::ShiftModifier)) {
        const QString action = event->modifiers() == Qt::ShiftModifier ? "stepBrushHardness" : "stepBrushSize";
        if (sendCommand({{"action", action}, {"enabled", rightBracket}})) {
            const auto brush = sessionState().value("brush").toObject();
            m_brushDiameter = qRound(brush.value("diameter").toDouble());
            m_brushHardness = qRound(100 * brush.value("hardness").toDouble());
            updateOptionsBar();
        }
        event->accept();
        return;
    }
    if (event->modifiers() == Qt::NoModifier) {
        switch (event->key()) {
        case Qt::Key_V:
            setTool(Tool::Move);
            event->accept();
            return;
        case Qt::Key_M:
            if (m_tool == Tool::Marquee) cycleMarqueeMode();
            else setTool(Tool::Marquee);
            event->accept();
            return;
        case Qt::Key_L:
            if (m_tool == Tool::Lasso) cycleLassoMode();
            else setTool(Tool::Lasso);
            event->accept();
            return;
        case Qt::Key_W:
            setTool(Tool::Magic);
            event->accept();
            return;
        case Qt::Key_Tab:
            sendCommand({{"action","cycleToolMode"}}); syncSharedControls(); updateOptionsBar(); event->accept(); return;
        case Qt::Key_C:
            setTool(Tool::Crop);
            event->accept();
            return;
        case Qt::Key_B:
            setBrushToolMode(BrushToolMode::Paint);
            setTool(Tool::Brush);
            event->accept();
            return;
        case Qt::Key_E:
            setBrushToolMode(BrushToolMode::Erase);
            setTool(Tool::Brush);
            event->accept();
            return;
        case Qt::Key_J:
            setTool(Tool::SpotHealing);
            event->accept();
            return;
        case Qt::Key_S:
            setTool(Tool::CloneStamp);
            event->accept();
            return;
        case Qt::Key_R:
            setTool(Tool::Smear);
            event->accept();
            return;
        case Qt::Key_G:
            setTool(Tool::Gradient);
            event->accept();
            return;
        case Qt::Key_U:
            setTool(Tool::Shape);
            event->accept();
            return;
        case Qt::Key_T:
            setTool(Tool::Type);
            event->accept();
            return;
        case Qt::Key_I:
            setTool(Tool::Eyedropper);
            event->accept();
            return;
        case Qt::Key_H:
            setTool(Tool::Hand);
            event->accept();
            return;
        case Qt::Key_Z:
            setTool(Tool::Zoom);
            event->accept();
            return;
        case Qt::Key_X:
            swapPaletteColors();
            event->accept();
            return;
        case Qt::Key_D:
            resetPaletteColors();
            event->accept();
            return;
        case Qt::Key_Space:
            if (!event->isAutoRepeat() && !m_spaceHandActive && m_tool != Tool::Hand) {
                m_preSpaceTool = m_tool;
                m_spaceHandActive = true;
                setTool(Tool::Hand);
                event->accept();
                return;
            }
            break;
        case Qt::Key_Escape:
            sendCommand({{"action","toolCancel"}}); m_polyActive=false; m_lassoPoints.clear(); m_painting=false; refreshImage(); updateOptionsBar();
            if (m_hasPendingCrop) {
                cancelCrop();
                event->accept();
                return;
            }
            break;
        case Qt::Key_Return:
        case Qt::Key_Enter:
            sendCommand({{"action","toolApply"}}); refreshImage(); updateOptionsBar();
            if (m_hasPendingCrop) {
                applyCrop();
                event->accept();
                return;
            }
            break;
        default:
            break;
        }
    } else if (event->modifiers() == Qt::ShiftModifier) {
        if (event->key() == Qt::Key_U) {
            cycleShapeMode();
            event->accept();
            return;
        }
    }
    QMainWindow::keyPressEvent(event);
}

void SessionWindow::keyReleaseEvent(QKeyEvent *event) {
    bool releaseHand=event->key()==Qt::Key_Space;
    for(auto v:m_shortcuts) {auto d=v.toObject(); if(d.value("title")=="Temporary Hand tool (hold)")
        releaseHand=event->key()==shortcuts::sequence(d.value("key").toString(),0)[0].key();}
    if (releaseHand && m_spaceHandActive && !event->isAutoRepeat()) {
        m_spaceHandActive = false;
        setTool(m_preSpaceTool);
        event->accept();
        return;
    }
    QMainWindow::keyReleaseEvent(event);
}

void SessionWindow::refreshMenuTitles(const QJsonObject &state) {
    const bool canUndo = state.value("canUndo").toBool(false);
    const QString undoName = state.value("undoName").toString();
    const bool canRedo = state.value("canRedo").toBool(false);
    const QString redoName = state.value("redoName").toString();
    const bool hasSelection = state.value("hasSelection").toBool(false);
    const bool canTransformSelection = state.value("canTransformSelection").toBool(false);
    const bool isMaskSelected = state.value("isMaskSelected").toBool(false);
    const bool activeLayerIsVisible = state.value("activeLayerIsVisible").toBool(true);
    const bool activeLayerHasMask = state.value("activeLayerHasMask").toBool(false);
    const bool activeLayerHasParent = state.value("activeLayerHasParent").toBool(false);
    const bool activeLayerIsClipped = state.value("activeLayerIsClipped").toBool(false);
    const bool canToggleClippingMask = state.value("canToggleClippingMask").toBool(false);
    const bool canMergeLayers = state.value("canMergeLayers").toBool(false);
    const QString mergeTitle = state.value("mergeTitle").toString("Merge Down");
    const bool canMoveActiveLayerUp = state.value("canMoveActiveLayerUp").toBool(false);
    const bool canMoveActiveLayerDown = state.value("canMoveActiveLayerDown").toBool(false);
    const QString activeLayerID = state.value("activeLayerID").toString();

    // 1. Undo / Redo
    if (m_actUndo) {
        m_actUndo->setEnabled(canUndo);
        m_actUndo->setText(canUndo && !undoName.isEmpty() ? QString("Undo %1").arg(undoName) : tr("Undo"));
    }
    if (m_actRedo) {
        m_actRedo->setEnabled(canRedo);
        m_actRedo->setText(canRedo && !redoName.isEmpty() ? QString("Redo %1").arg(redoName) : tr("Redo"));
    }

    // 2. Invert: isMaskSelected ? "Invert Mask" : "Invert"
    if (m_actInvert) {
        m_actInvert->setText(isMaskSelected ? tr("Invert Mask") : tr("Invert"));
    }

    // 3. Transform: canTransformSelection ? "Transform Selection" : "Transform Layer"
    if (m_actTransform) {
        m_actTransform->setText(canTransformSelection ? tr("Transform Selection") : tr("Transform Layer"));
    }

    // 4. Duplicate: selection == nil ? "Duplicate Layer" : "Layer via Copy"
    if (m_actDuplicate) {
        m_actDuplicate->setText(hasSelection ? tr("Layer via Copy") : tr("Duplicate Layer"));
    }

    // 5. Clipping Mask: activeLayerIsClipped ? "Release Clipping Mask" : "Create Clipping Mask"
    if (m_actClippingMask) {
        m_actClippingMask->setText(activeLayerIsClipped ? tr("Release Clipping Mask") : tr("Create Clipping Mask"));
        m_actClippingMask->setEnabled(canToggleClippingMask);
    }

    // 6. Layer Visibility: activeLayerIsVisible == false ? "Show Layer" : "Hide Layer"
    if (m_actShowHideLayer) {
        m_actShowHideLayer->setText(activeLayerIsVisible ? tr("Hide Layer") : tr("Show Layer"));
        m_actShowHideLayer->setEnabled(!activeLayerID.isEmpty());
    }

    // 7. Move out of folder
    if (m_actMoveOutOfFolder) {
        m_actMoveOutOfFolder->setEnabled(activeLayerHasParent);
    }

    // 8. Move Up / Down
    if (m_actMoveLayerUp) m_actMoveLayerUp->setEnabled(canMoveActiveLayerUp);
    if (m_actMoveLayerDown) m_actMoveLayerDown->setEnabled(canMoveActiveLayerDown);

    // 9. Merge: session.mergeTitle
    if (m_actMerge) {
        m_actMerge->setText(mergeTitle.isEmpty() ? tr("Merge Down") : mergeTitle);
        m_actMerge->setEnabled(canMergeLayers);
    }

    // 10. Delete: isMaskSelected && activeLayerHasMask ? "Delete Layer Mask" : (selectedLayers > 1 ? "Delete Layers" : "Delete Layer")
    if (m_actDelete) {
        int selectedCount = 1;
        if (m_layersView && m_layersView->selectionModel()) {
            selectedCount = std::max(1, static_cast<int>(m_layersView->selectionModel()->selectedRows().size()));
        }
        if (isMaskSelected && activeLayerHasMask) {
            m_actDelete->setText(tr("Delete Layer Mask"));
        } else if (selectedCount > 1) {
            m_actDelete->setText(tr("Delete Layers"));
        } else {
            m_actDelete->setText(tr("Delete Layer"));
        }
        m_actDelete->setEnabled(!activeLayerID.isEmpty());
    }

    // 11. Selection commands enable/disable
    if (m_actDeselect) m_actDeselect->setEnabled(hasSelection);
    if (m_actInverse) m_actInverse->setEnabled(hasSelection);
    if (m_actClearSelection) m_actClearSelection->setEnabled(hasSelection);
}

void SessionWindow::createMenus() {
    menuBar()->setNativeMenuBar(false);
    // --- File ---
    auto *file = menuBar()->addMenu(tr("&File"));
    file->addAction(tr("New Canvas…"), QKeySequence::New, this, [this] {
        if (sessionState().value("busy").toBool()) return;
        QDialog dialog(this);
        dialog.setObjectName("newCanvas.dialog"); dialog.setWindowTitle(tr("New Canvas"));
        auto *layout = new QFormLayout(&dialog);
        auto *width = new QSpinBox(&dialog), *height = new QSpinBox(&dialog);
        width->setObjectName("width"); height->setObjectName("height");
        width->setRange(1, 30000); height->setRange(1, 30000);
        width->setValue(1024); height->setValue(768);
        layout->addRow(tr("Width (pixels)"), width); layout->addRow(tr("Height (pixels)"), height);
        auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
        layout->addRow(buttons);
        auto validate = [=] { buttons->button(QDialogButtonBox::Ok)->setEnabled(qint64(width->value()) * height->value() <= 100000000); };
        connect(width, &QSpinBox::valueChanged, &dialog, validate);
        connect(height, &QSpinBox::valueChanged, &dialog, validate);
        connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
        connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
        if (dialog.exec() == QDialog::Accepted && confirmDocumentReplacement()) {
            createNewDocument(width->value(), height->value()); clearAutosave(); updateDocumentTitle();
        }
    })->setObjectName("file.new");

    file->addAction(tr("Open Project…"), QKeySequence::Open, this, [this] {
        const QString path = m_platform.files->chooseProjectToOpen();
        if (!path.isEmpty() && confirmDocumentReplacement()) {
            if (!loadProject(path)) m_platform.notifier->warn(tr("Open failed"), tr("Could not load project. Your current document is unchanged."));
            else clearAutosave();
        }
    })->setObjectName("file.openProject");

    file->addAction(tr("Import Images…"), this, [this] {
        const QString path = m_platform.files->chooseImageToOpen();
        if (!path.isEmpty() && !importImage(path, false))
            m_platform.notifier->warn(tr("Import failed"), tr("Could not read this image."));
    })->setObjectName("file.importImages");

    file->addSeparator();

    file->addAction(tr("Save"), QKeySequence::Save, this, [this] {
        saveCurrentProject();
    })->setObjectName("file.save");

    file->addAction(tr("Save As…"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_S), this, [this] {
        saveCurrentProject(true);
    })->setObjectName("file.saveAs");

    file->addSeparator();

    file->addAction(tr("Export PNG…"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_E), this, [this] {
        const QString path = m_platform.files->chooseExportPath(QStringLiteral("PNG"), tr("PNG (*.png)"));
        if (!path.isEmpty() && !exportPNG(path)) {
            m_platform.notifier->warn(tr("Export failed"), tr("Could not export PNG."));
        }
    })->setObjectName("file.exportPNG");

    file->addAction(tr("Export JPEG…"), QKeySequence(Qt::CTRL | Qt::ALT | Qt::SHIFT | Qt::Key_S), this, [this] {
        const QString path = m_platform.files->chooseExportPath(QStringLiteral("JPEG"), tr("JPEG (*.jpg *.jpeg)"));
        if (!path.isEmpty() && !exportJPEG(path)) {
            m_platform.notifier->warn(tr("Export failed"), tr("Could not export JPEG."));
        }
    })->setObjectName("file.exportJPEG");

    file->addAction(tr("Export TIFF…"), this, [this] {
        const QString path = m_platform.files->chooseExportPath(QStringLiteral("TIFF"), tr("TIFF (*.tiff *.tif)"));
        if (!path.isEmpty() && !exportTIFF(path)) {
            m_platform.notifier->warn(tr("Export failed"), tr("Could not export TIFF."));
        }
    })->setObjectName("file.exportTIFF");

    file->addAction(tr("Export WebP…"), this, [this] {
        const QString path = m_platform.files->chooseExportPath(QStringLiteral("WebP"), tr("WebP (*.webp)"));
        if (!path.isEmpty() && !exportWebP(path)) {
            m_platform.notifier->warn(tr("Export failed"), tr("Could not export WebP."));
        }
    })->setObjectName("file.exportWebP");

    file->addSeparator();

    file->addAction(tr("Close Project"), QKeySequence::Close, this, [this] {
        close();
    })->setObjectName("file.closeProject");

    file->addSeparator();

    file->addAction(tr("Quit"), QKeySequence::Quit, this, &QWidget::close)->setObjectName("file.quit");

    // --- Edit ---
    auto *edit = menuBar()->addMenu(tr("&Edit"));
    m_actUndo = edit->addAction(tr("Undo"), QKeySequence::Undo, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"undo"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actUndo->setObjectName("edit.undo");

    m_actRedo = edit->addAction(tr("Redo"), QKeySequence::Redo, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"redo"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actRedo->setObjectName("edit.redo");

    edit->addSeparator();

    m_actCut = edit->addAction(tr("Cut"), QKeySequence::Cut, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"cut"})") == 0) refreshImage();
    });
    m_actCut->setObjectName("edit.cut");

    m_actCopy = edit->addAction(tr("Copy"), QKeySequence::Copy, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"copy"})") == 0) {
            if (!m_image.isNull()) m_platform.clipboard->setImage(m_image);
            statusBar()->showMessage(tr("Copied to clipboard."), 1500);
        }
    });
    m_actCopy->setObjectName("edit.copy");

    m_actCopyMerged = edit->addAction(tr("Copy Merged"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_C), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"copyMerged"})") == 0) {
            if (!m_image.isNull()) m_platform.clipboard->setImage(m_image);
            statusBar()->showMessage(tr("Copied merged to clipboard."), 1500);
        }
    });
    m_actCopyMerged->setObjectName("edit.copyMerged");

    m_actPaste = edit->addAction(tr("Paste"), QKeySequence::Paste, this, [this] {
        const QImage img = m_platform.clipboard->image();
        if (!img.isNull()) {
            QImage rgba = img.convertToFormat(QImage::Format_RGBA8888);
            std::vector<uint8_t> premul(rgba.width() * rgba.height() * 4);
            for (int y = 0; y < rgba.height(); ++y) {
                const uint8_t *src = rgba.constScanLine(y);
                uint8_t *dst = premul.data() + y * rgba.width() * 4;
                for (int x = 0; x < rgba.width(); ++x) {
                    const uint8_t a = src[x * 4 + 3];
                    dst[x * 4] = static_cast<uint8_t>((src[x * 4] * a + 127) / 255);
                    dst[x * 4 + 1] = static_cast<uint8_t>((src[x * 4 + 1] * a + 127) / 255);
                    dst[x * 4 + 2] = static_cast<uint8_t>((src[x * 4 + 2] * a + 127) / 255);
                    dst[x * 4 + 3] = a;
                }
            }
            std::string name = "Pasted Layer";
            if (compositor_session_import_rgba(m_sessionHandle, premul.data(), premul.size(),
                                               rgba.width(), rgba.height(),
                                               reinterpret_cast<const uint8_t *>(name.data()), name.size(), 0) == 0) {
                refreshImage();
                refreshLayers();
            }
        }
    });
    m_actPaste->setObjectName("edit.paste");

    edit->addSeparator();

    edit->addAction(tr("Keyboard Shortcuts…"), this, [this] {
        shortcuts::edit(this,sessionState().value("shortcuts").toArray(),[this](QJsonObject values) {
            QJsonObject command{{"version",1},{"action","saveShortcuts"},{"shortcuts",values}};
            auto bytes=QJsonDocument(command).toJson();
            if(compositor_session_command(m_sessionHandle,reinterpret_cast<const uint8_t *>(bytes.constData()),bytes.size())!=0)
                return sessionState().value("error").toString("Invalid shortcut");
            for(auto *widget:QApplication::topLevelWidgets()) if(auto *window=qobject_cast<SessionWindow *>(widget)) window->applyShortcuts();
            return QString();
        });
    })->setObjectName("edit.shortcuts");

    edit->addSeparator();

    m_actFillFG = edit->addAction(tr("Fill with Foreground Color"), QKeySequence(Qt::ALT | Qt::Key_Delete), this, [this] {
        if (sendCommand({{"action", "fillForeground"}, {"parameters", QJsonObject{{"red", m_brushColor.redF()}, {"green", m_brushColor.greenF()}, {"blue", m_brushColor.blueF()}}}})) {
            refreshImage();
        }
    });
    m_actFillFG->setObjectName("fill.foreground");

    m_actFillBG = edit->addAction(tr("Fill with Background Color"), QKeySequence(Qt::CTRL | Qt::Key_Delete), this, [this] {
        if (sendCommand({{"action", "fillBackground"}, {"parameters", QJsonObject{{"red", m_backgroundColor.redF()}, {"green", m_backgroundColor.greenF()}, {"blue", m_backgroundColor.blueF()}}}})) {
            refreshImage();
        }
    });
    m_actFillBG->setObjectName("fill.background");

    m_actClearSelection = edit->addAction(tr("Clear Selection Pixels"), QKeySequence::Delete, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"clearSelection"})") == 0) refreshImage();
    });
    m_actClearSelection->setObjectName("fill.clear");

    m_actContentAwareFill = edit->addAction(tr("Content-Aware Fill…"), QKeySequence(Qt::SHIFT | Qt::Key_Delete), this, [this] {
        showFilterDialog("Content-Aware Fill");
    });
    m_actContentAwareFill->setObjectName("edit.contentFill");

    edit->addSeparator();

    edit->addAction(tr("Command Palette…"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_P), this, [this] {
        showCommandPalette();
    })->setObjectName("commandPalette");

    // --- View ---
    auto *view = menuBar()->addMenu(tr("&View"));
    view->addAction(tr("Fit Canvas"), QKeySequence(Qt::CTRL | Qt::Key_0), this, &SessionWindow::fitCanvas)->setObjectName("view.fitCanvas");
    view->addAction(tr("Actual Pixels"), QKeySequence(Qt::CTRL | Qt::Key_1), this, &SessionWindow::actualPixels)->setObjectName("view.actualPixels");
    view->addAction(tr("Zoom In"), QKeySequence::ZoomIn, this, [this] { zoomBy(1.25); })->setObjectName("view.zoomIn");
    view->addAction(tr("Zoom Out"), QKeySequence::ZoomOut, this, [this] { zoomBy(1.0 / 1.25); })->setObjectName("view.zoomOut");

    view->addSeparator();

    auto *actPixelGrid = view->addAction(tr("Pixel Grid (800% and above)"));
    actPixelGrid->setCheckable(true);
    actPixelGrid->setChecked(true);
    actPixelGrid->setObjectName("view.pixelGrid");

    auto *actSnap = view->addAction(tr("Snap"));
    actSnap->setCheckable(true);
    actSnap->setChecked(true);
    actSnap->setObjectName("view.snap");

    auto *actTransformControls = view->addAction(tr("Show Transform Controls"), QKeySequence(Qt::CTRL | Qt::Key_H), this, [this](bool) {
        if (m_canvasWidget) m_canvasWidget->update();
    });
    actTransformControls->setCheckable(true);
    actTransformControls->setChecked(true);
    actTransformControls->setObjectName("view.transformControls");

    view->addSeparator();

    auto *showMenu = view->addMenu(tr("Show"));
    auto *actShowGrid = showMenu->addAction(tr("Grid"), QKeySequence(Qt::CTRL | Qt::Key_Apostrophe));
    actShowGrid->setCheckable(true);
    actShowGrid->setObjectName("view.showGrid");
    auto *actShowGuides = showMenu->addAction(tr("Guides"), QKeySequence(Qt::CTRL | Qt::Key_Semicolon));
    actShowGuides->setCheckable(true);
    actShowGuides->setChecked(true);
    actShowGuides->setObjectName("view.showGuides");

    auto *actRulers = view->addAction(tr("Rulers"), QKeySequence(Qt::CTRL | Qt::Key_R));
    actRulers->setCheckable(true);
    actRulers->setObjectName("view.rulers");

    view->addSeparator();

    auto *snapMenu = view->addMenu(tr("Snap To"));
    auto *snapGuides = snapMenu->addAction(tr("Guides")); snapGuides->setCheckable(true); snapGuides->setChecked(true);
    auto *snapGrid = snapMenu->addAction(tr("Grid")); snapGrid->setCheckable(true);
    auto *snapLayers = snapMenu->addAction(tr("Layers")); snapLayers->setCheckable(true); snapLayers->setChecked(true);
    auto *snapDocBounds = snapMenu->addAction(tr("Document Bounds")); snapDocBounds->setCheckable(true); snapDocBounds->setChecked(true);

    view->addSeparator();

    auto *actLockGuides = view->addAction(tr("Lock Guides"), QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_Semicolon));
    actLockGuides->setCheckable(true);
    actLockGuides->setObjectName("view.lockGuides");

    view->addAction(tr("Clear Guides"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"clearGuides"})") == 0) {
            if (m_canvasWidget) m_canvasWidget->update();
        }
    })->setObjectName("view.clearGuides");

    // --- Select ---
    auto *select = menuBar()->addMenu(tr("&Select"));
    m_actSelectAll = select->addAction(tr("All"), QKeySequence::SelectAll, this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"selectAll"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actSelectAll->setObjectName("select.all");

    m_actDeselect = select->addAction(tr("Deselect"), QKeySequence(Qt::CTRL | Qt::Key_D), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"deselect"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actDeselect->setObjectName("select.deselect");

    m_actInverse = select->addAction(tr("Inverse"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_I), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"invertSelection"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actInverse->setObjectName("select.inverse");

    m_actLayerPixels = select->addAction(tr("Layer's Pixels"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"loadLayerSelection"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actLayerPixels->setObjectName("select.layerPixels");

    m_actSelectSubject = select->addAction(tr("Subject"), QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_A), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"selectSubject"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actSelectSubject->setObjectName("select.subject");

    m_actMaskBlackAreas = select->addAction(tr("Mask's Black Areas"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"loadMaskSelection"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actMaskBlackAreas->setObjectName("select.maskBlackAreas");

    select->addSeparator();

    m_actExpandSelection = select->addAction(tr("Expand…"), this, [this] {
        bool ok = false;
        const int px = QInputDialog::getInt(this, tr("Expand Selection"), tr("Expand by (pixels):"), 1, 1, 500, 1, &ok);
        if (ok && px > 0) {
            if (sendCommand({{"action", "expandSelection"}, {"parameters", QJsonObject{{"amount", px}}}})) {
                refreshImage();
                refreshLayers();
            }
        }
    });
    m_actExpandSelection->setObjectName("select.expand");

    m_actContractSelection = select->addAction(tr("Contract…"), this, [this] {
        bool ok = false;
        const int px = QInputDialog::getInt(this, tr("Contract Selection"), tr("Contract by (pixels):"), 1, 1, 500, 1, &ok);
        if (ok && px > 0) {
            if (sendCommand({{"action", "contractSelection"}, {"parameters", QJsonObject{{"amount", px}}}})) {
                refreshImage();
                refreshLayers();
            }
        }
    });
    m_actContractSelection->setObjectName("select.contract");

    m_actFeatherSelection = select->addAction(tr("Feather…"), this, [this] {
        bool ok = false;
        const int px = QInputDialog::getInt(this, tr("Feather Selection"), tr("Feather radius (pixels):"), 1, 1, 500, 1, &ok);
        if (ok && px > 0) {
            if (sendCommand({{"action", "featherSelection"}, {"parameters", QJsonObject{{"amount", px}}}})) {
                refreshImage();
                refreshLayers();
            }
        }
    });
    m_actFeatherSelection->setObjectName("select.feather");

    select->addSeparator();

    // Preserved for automation compatibility:
    select->addAction(tr("Rectangle Selection"), this, [this] { selectRegion(true); })->setObjectName("select.rectangle");
    select->addAction(tr("Ellipse Selection"), this, [this] { selectRegion(false); })->setObjectName("select.ellipse");

    // --- Image ---
    auto *image = menuBar()->addMenu(tr("&Image"));
    image->addAction(tr("Curves…"), QKeySequence(Qt::CTRL | Qt::Key_M), this, [this] { showAdjustDialog("Curves"); })->setObjectName("adjust.Curves");
    image->addAction(tr("Levels…"), QKeySequence(Qt::CTRL | Qt::Key_L), this, [this] { showAdjustDialog("Levels"); })->setObjectName("adjust.Levels");
    image->addAction(tr("Hue/Saturation…"), QKeySequence(Qt::CTRL | Qt::Key_U), this, [this] { showAdjustDialog("Hue/Saturation"); })->setObjectName("adjust.Hue/Saturation");
    for (const QString &kind : {QString("Black & White"), QString("Color Balance"), QString("Exposure"), QString("Gradient Map"), QString("Grain")}) {
        auto *act = image->addAction(kind + "…", this, [this, kind] { showAdjustDialog(kind); });
        act->setObjectName("adjust." + kind);
    }

    image->addSeparator();

    m_actInvert = image->addAction(tr("Invert"), QKeySequence(Qt::CTRL | Qt::Key_I), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"invert"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actInvert->setObjectName("image.invert");

    image->addSeparator();

    image->addAction(tr("Canvas Size…"), QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_C), this, [this] { showSizeDialog(false); })->setObjectName("canvasSize");
    image->addAction(tr("Image Size…"), QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_I), this, [this] { showSizeDialog(true); })->setObjectName("imageSize");

    image->addSeparator();

    image->addAction(tr("Flip Canvas Horizontal"), this, [this] {
        if (sendCommand({{"action", "flipCanvas"}, {"horizontally", true}})) refreshImage();
    })->setObjectName("image.flipH");
    image->addAction(tr("Flip Canvas Vertical"), this, [this] {
        if (sendCommand({{"action", "flipCanvas"}, {"horizontally", false}})) refreshImage();
    })->setObjectName("image.flipV");

    // --- Filter ---
    auto *filter = menuBar()->addMenu(tr("&Filter"));
    for (const QString &kind : {QString("Gaussian Blur"), QString("Motion Blur"), QString("Add Noise"), QString("Lens Correction")}) {
        auto *action = filter->addAction(kind + "…", this, [this, kind] { showFilterDialog(kind); });
        action->setObjectName("filter." + kind);
    }
    filter->addAction(tr("Remove Background…"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"removeBackground"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    })->setObjectName("filter.Remove Background");

    // --- Layer ---
    auto *layer = menuBar()->addMenu(tr("&Layer"));
    auto *adjMenu = layer->addMenu(tr("New Adjustment Layer"));
    for (const QString &kind : {QString("Hue/Saturation"), QString("Levels"), QString("Curves"), QString("Exposure"),
                                QString("Gradient Map"), QString("Grain"), QString("Invert"), QString("Black & White"), QString("Color Balance")}) {
        adjMenu->addAction(kind == "Invert" ? kind : kind + "…", this, [this, kind] {
            showAdjustDialog(kind);
        })->setObjectName("layer.adjust." + kind);
    }

    layer->addAction(tr("Edit Adjustment…"), this, [this] {
        const QJsonObject state = sessionState();
        const QString active = state.value("activeLayerID").toString();
        for (const QJsonValue &v : state.value("layers").toArray()) {
            const QJsonObject l = v.toObject();
            if (l.value("id").toString() == active && l.contains("adjustment")) {
                const QString kind = l.value("adjustment").toObject().value("kind").toString();
                if (!kind.isEmpty()) showAdjustDialog(kind);
                break;
            }
        }
    })->setObjectName("layer.editAdjustment");

    layer->addSeparator();

    m_actTransform = layer->addAction(tr("Transform Layer"), QKeySequence(Qt::CTRL | Qt::Key_T), this, [this] {
        setTool(Tool::Move);
    });
    m_actTransform->setObjectName("layer.transform");

    m_actDuplicate = layer->addAction(tr("Duplicate Layer"), QKeySequence(Qt::CTRL | Qt::Key_J), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"duplicateLayer"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actDuplicate->setObjectName("layer.duplicate");

    layer->addSeparator();

    m_actClippingMask = layer->addAction(tr("Create Clipping Mask"), QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_G), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"toggleClippingMask"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actClippingMask->setObjectName("layer.clippingMask");

    layer->addSeparator();

    m_actGroupLayers = layer->addAction(tr("Group Selected Layers"), QKeySequence(Qt::CTRL | Qt::Key_G), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"groupSelectedLayers"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actGroupLayers->setObjectName("layer.addGroup");

    m_actMoveOutOfFolder = layer->addAction(tr("Move Out of Folder"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"moveActiveLayerOutOfGroup"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actMoveOutOfFolder->setObjectName("layer.moveOutOfFolder");

    m_actNewBlankLayer = layer->addAction(tr("New Blank Layer"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_N), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"addLayer"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actNewBlankLayer->setObjectName("layer.new");

    m_actRenameLayer = layer->addAction(tr("Rename Layer…"), this, [this] {
        if (m_layersView) m_layersView->edit(m_layersView->currentIndex());
    });
    m_actRenameLayer->setObjectName("layer.rename");

    m_actShowHideLayer = layer->addAction(tr("Show Layer"), this, [this] {
        const QJsonObject state = sessionState();
        const bool vis = state.value("activeLayerIsVisible").toBool(true);
        if (setLayerFlag("setVisible", !vis)) refreshImage();
    });
    m_actShowHideLayer->setObjectName("layer.showHide");

    layer->addSeparator();

    m_actMoveLayerUp = layer->addAction(tr("Move Layer Up"), QKeySequence(Qt::CTRL | Qt::Key_BracketRight), this, [this] {
        if (sendCommand({{"action", "moveActiveLayer"}, {"value", 1}})) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actMoveLayerUp->setObjectName("layer.moveUp");

    m_actMoveLayerDown = layer->addAction(tr("Move Layer Down"), QKeySequence(Qt::CTRL | Qt::Key_BracketLeft), this, [this] {
        if (sendCommand({{"action", "moveActiveLayer"}, {"value", -1}})) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actMoveLayerDown->setObjectName("layer.moveDown");

    layer->addSeparator();

    m_actMerge = layer->addAction(tr("Merge Down"), QKeySequence(Qt::CTRL | Qt::Key_E), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"mergeLayers"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    });
    m_actMerge->setObjectName("layer.merge");

    layer->addSeparator();

    m_actFlipLayerH = layer->addAction(tr("Flip Layer Horizontal"), this, [this] {
        if (sendCommand({{"action", "flipLayer"}, {"horizontally", true}})) refreshImage();
    });
    m_actFlipLayerH->setObjectName("layer.flipH");

    m_actFlipLayerV = layer->addAction(tr("Flip Layer Vertical"), this, [this] {
        if (sendCommand({{"action", "flipLayer"}, {"horizontally", false}})) refreshImage();
    });
    m_actFlipLayerV->setObjectName("layer.flipV");

    layer->addSeparator();

    auto *maskMenu = layer->addMenu(tr("Layer Mask"));
    maskMenu->addAction(tr("Add Reveal Mask"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"addRevealMask"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    })->setObjectName("layer.addRevealMask");

    maskMenu->addAction(tr("Add Hide Mask"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"addHideMask"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    })->setObjectName("layer.addHideMask");

    maskMenu->addAction(tr("Invert Mask"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"invertMask"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    })->setObjectName("layer.invertMask");

    maskMenu->addAction(tr("Delete Mask"), this, [this] {
        if (cmd(m_sessionHandle, R"({"version":1,"action":"deleteMask"})") == 0) {
            refreshImage();
            refreshLayers();
        }
    })->setObjectName("layer.deleteMask");

    layer->addSeparator();

    m_actDelete = layer->addAction(tr("Delete Layer"), QKeySequence::Delete, this, [this] {
        deleteSelectedLayers();
    });
    m_actDelete->setObjectName("layer.delete");

    // --- Window ---
    auto *window = menuBar()->addMenu(tr("&Window"));
    window->addAction(tr("Reset Workspace Layout"), this, [this] {
        if (m_toolsBar) m_toolsBar->show();
        if (m_optionsToolBar) m_optionsToolBar->show();
        if (m_layersDock) m_layersDock->show();
    })->setObjectName("window.resetLayout");

    window->addSeparator();

    if (m_toolsBar) {
        auto *actTools = window->addAction(tr("Tools"));
        actTools->setCheckable(true);
        actTools->setChecked(m_toolsBar->isVisible());
        connect(actTools, &QAction::toggled, m_toolsBar, &QWidget::setVisible);
        connect(m_toolsBar, &QToolBar::visibilityChanged, actTools, &QAction::setChecked);
    }
    if (m_optionsToolBar) {
        auto *actOptions = window->addAction(tr("Options Bar"));
        actOptions->setCheckable(true);
        actOptions->setChecked(m_optionsToolBar->isVisible());
        connect(actOptions, &QAction::toggled, m_optionsToolBar, &QWidget::setVisible);
        connect(m_optionsToolBar, &QToolBar::visibilityChanged, actOptions, &QAction::setChecked);
    }
    if (m_layersDock) {
        auto *actLayers = window->addAction(tr("Layers"));
        actLayers->setCheckable(true);
        actLayers->setChecked(m_layersDock->isVisible());
        connect(actLayers, &QAction::toggled, m_layersDock, &QWidget::setVisible);
        connect(m_layersDock, &QDockWidget::visibilityChanged, actLayers, &QAction::setChecked);
    }
    if (m_adjustmentsDock) {
        window->addAction(m_adjustmentsDock->toggleViewAction());
    }

    // --- Help ---
    auto *help = menuBar()->addMenu(tr("&Help"));
    help->addAction(tr("About Compositor"), this, [this] {
        QMessageBox::about(this, tr("About Compositor"),
            tr("<h3>Compositor</h3>"
               "<p>Professional Non-Destructive Image Editor.</p>"
               "<p>Native GNU/Linux port powered by Qt 6, Skia, and unmodified upstream Swift engine.</p>"));
    })->setObjectName("help.about");

    help->addAction(tr("Check for Updates…"), this, [this] {
        if (m_platform.updates) {
            m_platform.updates->checkForUpdates(true);
        } else {
            QMessageBox::information(this, tr("Check for Updates"),
                tr("You are running the latest version of Compositor for GNU/Linux."));
        }
    })->setObjectName("help.updates");
}

namespace {
constexpr double kPi = 3.14159265358979323846;
const QPointF kHandleUnits[8] = {{0, 0}, {0.5, 0}, {1, 0}, {1, 0.5}, {1, 1}, {0.5, 1}, {0, 1}, {0, 0.5}};

QPointF geometryPoint(const SessionWindow::LayerGeometry &g, const QPointF &unit) {
    const double r = g.rotation * kPi / 180.0;
    const double lx = (unit.x() - 0.5) * g.w, ly = (unit.y() - 0.5) * g.h;
    return QPointF(g.x + g.w / 2 + lx * std::cos(r) - ly * std::sin(r),
                   g.y + g.h / 2 + lx * std::sin(r) + ly * std::cos(r));
}

bool geometryContains(const SessionWindow::LayerGeometry &g, const QPointF &p) {
    const double r = g.rotation * kPi / 180.0;
    const double x = p.x() - (g.x + g.w / 2), y = p.y() - (g.y + g.h / 2);
    return std::abs(x * std::cos(r) + y * std::sin(r)) <= g.w / 2
        && std::abs(-x * std::sin(r) + y * std::cos(r)) <= g.h / 2;
}
}  // namespace

QRectF SessionWindow::canvasTargetRect() const {
    if (m_image.isNull()) return QRectF();
    const QSize canvasSize = m_canvasWidget ? m_canvasWidget->size() : size();
    const int pad = 24;
    const int maxW = std::max(10, canvasSize.width() - pad * 2);
    const int maxH = std::max(10, canvasSize.height() - pad * 2);
    double scale = 1.0;
    if (m_zoomLevel > 0.0) {
        scale = m_zoomLevel;
    } else {
        scale = std::min(static_cast<double>(maxW) / m_image.width(),
                         static_cast<double>(maxH) / m_image.height());
        if (m_image.width() <= 128 && m_image.height() <= 128) {
            int intScale = std::max(1, static_cast<int>(scale));
            scale = intScale;
        }
    }
    const double displayW = m_image.width() * scale;
    const double displayH = m_image.height() * scale;
    return QRectF((canvasSize.width() - displayW) / 2.0 + m_panOffset.x(),
                  (canvasSize.height() - displayH) / 2.0 + m_panOffset.y(),
                  displayW, displayH);
}

QPointF SessionWindow::documentToCanvasPoint(const QPointF &docPoint) const {
    const QRectF target = canvasTargetRect();
    if (m_image.isNull() || target.width() <= 0 || target.height() <= 0) return docPoint;
    return QPointF(target.left() + docPoint.x() * target.width() / m_image.width(),
                   target.top() + docPoint.y() * target.height() / m_image.height());
}

QPointF SessionWindow::documentPoint(const QPointF &windowPoint) const {
    if (m_image.isNull()) return QPointF();
    const QRectF target = canvasTargetRect();
    if (target.width() <= 0 || target.height() <= 0) return QPointF();
    const QPointF local = windowPoint - target.topLeft();
    return QPointF(local.x() * m_image.width() / target.width(),
                   local.y() * m_image.height() / target.height());
}

void SessionWindow::canvasPaintEvent(QPaintEvent *event, QWidget *canvas) {
    Q_UNUSED(event);
    QPainter p(canvas);
    // Dark professional neutral workspace background
    p.fillRect(canvas->rect(), QColor(0x24, 0x25, 0x28));

    if (m_image.isNull()) return;

    const QRectF target = canvasTargetRect();
    const QRect targetI = target.toRect();

    // Subtle drop shadow around document canvas
    p.fillRect(targetI.adjusted(2, 2, 4, 4), QColor(0, 0, 0, 90));

    // Transparent checkerboard pattern
    static QPixmap checker;
    if (checker.isNull()) {
        QImage chk(16, 16, QImage::Format_RGB32);
        QPainter cp(&chk);
        cp.fillRect(0, 0, 8, 8, QColor(0xee, 0xee, 0xee));
        cp.fillRect(8, 8, 8, 8, QColor(0xee, 0xee, 0xee));
        cp.fillRect(8, 0, 8, 8, QColor(0xcc, 0xcc, 0xcc));
        cp.fillRect(0, 8, 8, 8, QColor(0xcc, 0xcc, 0xcc));
        checker = QPixmap::fromImage(chk);
    }
    p.drawTiledPixmap(targetI, checker);

    if (target.width() >= m_image.width()) {
        p.setRenderHint(QPainter::SmoothPixmapTransform, false);
    } else {
        p.setRenderHint(QPainter::SmoothPixmapTransform, true);
    }
    p.drawImage(target, m_image);

    // Canvas border outline
    p.setPen(QPen(QColor(0x10, 0x10, 0x10), 1));
    p.drawRect(targetI.adjusted(0, 0, -1, -1));

    drawTransformControls(p);
    drawCropOverlay(p);

    if (m_tool == Tool::Lasso && !m_lassoPoints.empty()) {
        QPolygonF outline;
        for (const QPointF &pt : m_lassoPoints) outline << documentToCanvasPoint(pt);
        if (m_polygonalLasso && m_polyActive) outline << documentToCanvasPoint(m_currentPoint);
        p.save();
        p.setRenderHint(QPainter::Antialiasing, true);
        p.setPen(QPen(Qt::white, 1, Qt::DashLine));
        p.drawPolyline(outline);
        if (m_polygonalLasso) {
            p.setPen(QPen(QColor(0x10, 0x10, 0x10), 1)); p.setBrush(Qt::white);
            for (const QPointF &pt : m_lassoPoints) p.drawRect(QRectF(documentToCanvasPoint(pt) - QPointF(3, 3), QSizeF(6, 6)));
        }
        p.restore();
    }

    // Interactive drag feedback (selection marquee)
    if (m_painting && (m_tool == Tool::Marquee || m_tool == Tool::Shape)) {
        const QPointF startCanvas = documentToCanvasPoint(m_dragStart);
        const QPointF currentCanvas = documentToCanvasPoint(m_currentPoint);
        QRectF selRect(startCanvas, currentCanvas);
        selRect = selRect.normalized();
        p.setPen(QPen(Qt::white, 1, Qt::DashLine));
        if (m_tool==Tool::Shape && m_shapeMode==ShapeMode::Line) p.drawLine(startCanvas,currentCanvas);
        else if ((m_tool==Tool::Marquee && m_marqueeMode == MarqueeMode::Ellipse) || (m_tool==Tool::Shape && m_shapeMode==ShapeMode::Ellipse)) {
            p.drawEllipse(selRect);
        } else {
            p.drawRect(selRect);
        }
    }
}

void SessionWindow::canvasMousePressEvent(QMouseEvent *event, QWidget *canvas) {
    Q_UNUSED(canvas);
    m_currentPoint = documentPoint(event->position());
    mousePressEvent(event);
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::canvasMouseMoveEvent(QMouseEvent *event, QWidget *canvas) {
    Q_UNUSED(canvas);
    m_currentPoint = documentPoint(event->position());
    mouseMoveEvent(event);
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::canvasMouseReleaseEvent(QMouseEvent *event, QWidget *canvas) {
    Q_UNUSED(canvas);
    m_currentPoint = documentPoint(event->position());
    mouseReleaseEvent(event);
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::canvasTabletEvent(QTabletEvent *event, QWidget *canvas) {
    Q_UNUSED(canvas);
    tabletEvent(event);
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::canvasDragEnterEvent(QDragEnterEvent *event, QWidget *canvas) {
    Q_UNUSED(canvas);
    dragEnterEvent(event);
}

void SessionWindow::canvasDropEvent(QDropEvent *event, QWidget *canvas) {
    Q_UNUSED(canvas);
    dropEvent(event);
}

void SessionWindow::paintEvent(QPaintEvent *event) {
    if (m_canvasWidget) {
        QMainWindow::paintEvent(event);
    } else {
        canvasPaintEvent(event, this);
    }
}

QJsonObject SessionWindow::sessionState() const {
    const int64_t size = compositor_session_state(m_sessionHandle, nullptr, 0);
    if (size <= 0 || size > 4 * 1024 * 1024) return {};
    QByteArray bytes(static_cast<qsizetype>(size), Qt::Uninitialized);
    if (compositor_session_state(m_sessionHandle, reinterpret_cast<uint8_t *>(bytes.data()), bytes.size()) != size) return {};
    return QJsonDocument::fromJson(bytes).object();
}

bool SessionWindow::sendCommand(const QJsonObject &command) {
    if (m_sessionHandle == 0) return false;
    QJsonObject payload = command;
    payload.insert("version", 1);
    const QByteArray bytes = QJsonDocument(payload).toJson(QJsonDocument::Compact);
    const int result = compositor_session_command(m_sessionHandle,
        reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size());
    if (result != 0) statusBar()->showMessage(sessionState().value("error").toString(tr("Could not apply the operation.")), 5000);
    return result == 0;
}

void SessionWindow::refreshImage() {
    if (m_sessionHandle == 0) return;
    const auto state = sessionState();
    updateDocumentTitle();
    const int width = state.value("width").toInt(), height = state.value("height").toInt();
    if (width <= 0 || height <= 0 || qint64(width) * height > 100000000) return;
    QImage rendered = renderToQImage(m_sessionHandle, width, height);
    if (rendered.isNull()) return;
    const int dpm = qRound(state.value("resolution").toDouble(72) / 0.0254);
    rendered.setDotsPerMeterX(dpm);
    rendered.setDotsPerMeterY(dpm);
    m_image = rendered;
    if (m_canvasWidget) m_canvasWidget->update();
    update();
    updateStatusTelemetry();
    QMetaObject::invokeMethod(this, [this] { refreshLayers(); }, Qt::QueuedConnection);
    // A completed canvas selection enables the shared Expand/Contract controls.
    if (!m_painting && (m_tool == Tool::Marquee || m_tool == Tool::Lasso || m_tool == Tool::Magic)) updateOptionsBar();
}

void SessionWindow::selectRegion(bool rectangle) {
    QJsonObject command{{"action", rectangle ? "selectRectangle" : "selectEllipse"},
                        {"x", 0}, {"y", 0}, {"width", m_image.width()}, {"height", m_image.height()}};
    if (sendCommand(command)) refreshImage();
}

bool SessionWindow::setLayerFlag(const char *action, bool on) {
    QJsonObject command{{"action", action}, {"enabled", on}};
    return sendCommand(command);
}

void SessionWindow::refreshLayers() {
    if (!m_layersView || !m_layerModel || m_sessionHandle == 0) return;
    const int64_t size = compositor_session_state(m_sessionHandle, nullptr, 0);
    if (size <= 0 || size > 4 * 1024 * 1024) return;
    std::vector<uint8_t> bytes(static_cast<size_t>(size));
    if (compositor_session_state(m_sessionHandle, bytes.data(), bytes.size()) != size) return;
    const QJsonDocument state = QJsonDocument::fromJson(QByteArray(reinterpret_cast<char *>(bytes.data()), bytes.size()));
    if (!state.isObject()) return;
    const QJsonArray layers = state.object().value("layers").toArray();
    const QString active = state.object().value("activeLayerID").toString();

    m_layerGeometries.clear();
    m_activeGeometry = LayerGeometry();
    for (const QJsonValue &value : layers) {
        const QJsonObject layer = value.toObject();
        const QJsonObject t = layer.value("transform").toObject();
        LayerGeometry g;
        g.id = layer.value("id").toString();
        g.isGroup = layer.value("isGroup").toBool(false);
        g.visible = layer.value("visible").toBool(true);
        // Foundation encodes CGPoint/CGSize as [a, b]; accept keyed objects as well.
        auto pair = [](const QJsonValue &v, const char *k0, const char *k1) {
            if (v.isArray()) return QPointF(v.toArray().at(0).toDouble(), v.toArray().at(1).toDouble());
            return QPointF(v.toObject().value(k0).toDouble(), v.toObject().value(k1).toDouble());
        };
        const QPointF origin = pair(t.value("origin"), "x", "y");
        const QPointF extent = pair(t.value("size"), "width", "height");
        g.x = origin.x(); g.y = origin.y(); g.w = extent.x(); g.h = extent.y();
        g.rotation = t.value("rotation").toDouble();
        g.valid = !g.isGroup && g.w >= 1 && g.h >= 1;
        m_layerGeometries.push_back(g);
        if (g.id == active) m_activeGeometry = g;
    }
    syncTransformFields();

    QSet<QString> collapsed;
    std::function<void(QModelIndex)> remember=[&](QModelIndex parent) {
        for(int row=0;row<m_layerModel->rowCount(parent);++row) {
            auto idx=m_layerModel->index(row,0,parent);
            if(idx.data(Qt::UserRole+1).toBool() && !m_layersView->isExpanded(idx)) collapsed.insert(idx.data(Qt::UserRole).toString());
            remember(idx);
        }
    }; remember({});
    m_syncingLayers = true;
    m_layerModel->clear();
    m_layerModel->setHorizontalHeaderLabels({tr("Layer"), tr("Visible"), tr("Mask")});

    struct LayerNode {
        QList<QStandardItem *> items;
        QString id;
        QString parentId;
        bool isGroup = false;
        bool hasMask = false;
        bool maskEnabled = false;
        bool visible = true;
        double opacity = 1.0;
    };

    QMap<QString, LayerNode> nodeMap;
    QList<QString> order;

    for (int i = 0; i < layers.size(); ++i) {
        const QJsonObject layer = layers.at(i).toObject();
        const QString id = layer.value("id").toString();
        const QString name = layer.value("name").toString();
        const QString parentId = layer.value("parentID").toString();
        const bool isGroup = layer.value("isGroup").toBool(false);
        const bool visible = layer.value("visible").toBool(true);
        const bool hasMask = layer.value("hasMask").toBool(false);
        const bool maskEnabled = hasMask && layer.value("maskEnabled").toBool(true);
        const double opacity = layer.value("opacity").toDouble(1.0);

        auto *nameItem = new QStandardItem(name);
        nameItem->setEditable(true);
        nameItem->setDropEnabled(isGroup);
        nameItem->setData(id, Qt::UserRole);
        nameItem->setData(isGroup, Qt::UserRole + 1);
        nameItem->setData(hasMask, Qt::UserRole + 2);
        nameItem->setData(maskEnabled, Qt::UserRole + 3);
        nameItem->setData(parentId, Qt::UserRole + 4);

        if (isGroup) {
            nameItem->setData(tr("Folder"), LayerItemDelegate::SubtitleRole);
            nameItem->setToolTip(tr("Group / Folder"));
        } else {
            nameItem->setToolTip(tr("Raster Layer"));
            const QByteArray idBytes = id.toUtf8();
            size_t assetW = 0, assetH = 0;
            const int64_t assetBytes = compositor_session_export_layer(
                m_sessionHandle, reinterpret_cast<const uint8_t *>(idBytes.constData()), idBytes.size(), 0,
                nullptr, 0, &assetW, &assetH);
            if (assetBytes > 0 && assetW > 0 && assetH > 0 && layers.size() <= 64
                && static_cast<size_t>(assetBytes) == assetW * assetH * 4) {
                nameItem->setData(tr("%1 × %2 px").arg(assetW).arg(assetH), LayerItemDelegate::SubtitleRole);
                std::vector<uint8_t> pixels(static_cast<size_t>(assetBytes));
                if (compositor_session_export_layer(m_sessionHandle, reinterpret_cast<const uint8_t *>(idBytes.constData()),
                                                    idBytes.size(), 0, pixels.data(), pixels.size(), &assetW, &assetH) == assetBytes) {
                    const QImage full(pixels.data(), static_cast<int>(assetW), static_cast<int>(assetH),
                                      static_cast<qsizetype>(assetW * 4), QImage::Format_RGBA8888_Premultiplied);
                    nameItem->setData(QPixmap::fromImage(full.scaled(72, 72, Qt::KeepAspectRatio, Qt::FastTransformation)),
                                      LayerItemDelegate::ThumbnailRole);
                }
            } else {
                nameItem->setData(tr("%1 × %2 px").arg(state.object().value("width").toInt()).arg(state.object().value("height").toInt()),
                                  LayerItemDelegate::SubtitleRole);
            }
        }

        auto *visItem = new QStandardItem();
        visItem->setEditable(false);
        visItem->setCheckable(true);
        visItem->setCheckState(visible ? Qt::Checked : Qt::Unchecked);
        visItem->setText(visible ? tr("Visible") : tr("Hidden"));

        auto *maskItem = new QStandardItem();
        maskItem->setEditable(false);
        if (hasMask) {
            maskItem->setCheckable(true);
            maskItem->setCheckState(maskEnabled ? Qt::Checked : Qt::Unchecked);
            maskItem->setText(maskEnabled ? tr("Mask: On") : tr("Mask: Off"));
            maskItem->setToolTip(maskEnabled ? tr("Raster mask active") : tr("Raster mask disabled"));
        } else {
            maskItem->setCheckable(false);
            maskItem->setText(QStringLiteral("—"));
            maskItem->setEnabled(false);
        }

        LayerNode node;
        node.items = {nameItem, visItem, maskItem};
        node.id = id;
        node.parentId = parentId;
        node.isGroup = isGroup;
        node.hasMask = hasMask;
        node.maskEnabled = maskEnabled;
        node.visible = visible;
        node.opacity = opacity;

        nodeMap.insert(id, node);
        order.append(id);
    }

    QModelIndex selectedIndex;
    QJsonObject activeLayerObj;

    // The document array is bottom-first; the panel lists the top layer first.
    for (auto it = order.crbegin(); it != order.crend(); ++it) {
        const QString &id = *it;
        const LayerNode &node = nodeMap[id];
        if (!node.parentId.isEmpty() && nodeMap.contains(node.parentId)) {
            QStandardItem *parentItem = nodeMap[node.parentId].items.at(0);
            parentItem->appendRow(node.items);
        } else {
            m_layerModel->invisibleRootItem()->appendRow(node.items);
        }

        if (id == active) {
            selectedIndex = node.items.at(0)->index();
            activeLayerObj.insert("opacity", node.opacity);
            activeLayerObj.insert("visible", node.visible);
            activeLayerObj.insert("hasMask", node.hasMask);
            activeLayerObj.insert("maskEnabled", node.maskEnabled);
        }
    }

    if(nodeMap.contains(active)) selectedIndex=nodeMap[active].items.at(0)->index();
    m_layersView->expandAll();
    for(const auto &id:collapsed) if(nodeMap.contains(id)) m_layersView->collapse(nodeMap[id].items.at(0)->index());
    m_layersView->setColumnHidden(1, true);
    m_layersView->setColumnHidden(2, true);

    if (m_layerCountLabel) {
        m_layerCountLabel->setText(QString::number(order.size()));
    }

    if (selectedIndex.isValid()) {
        m_layersView->setCurrentIndex(selectedIndex);
        m_layersView->selectionModel()->clearSelection();
        for(auto id:state.object().value("selectedLayerIDs").toArray()) if(nodeMap.contains(id.toString()))
            m_layersView->selectionModel()->select(nodeMap[id.toString()].items.at(0)->index(),QItemSelectionModel::Select|QItemSelectionModel::Rows);
        m_layersView->scrollTo(selectedIndex);

        const int opVal = qRound(activeLayerObj.value("opacity").toDouble(1.0) * 100);
        m_opacity->setValue(opVal);
        if (m_opacityLabel) m_opacityLabel->setText(QString("%1 %").arg(opVal));
        m_visibleCheck->setChecked(activeLayerObj.value("visible").toBool(true));
        m_maskCheck->setEnabled(activeLayerObj.value("hasMask").toBool(false));
        m_maskCheck->setVisible(activeLayerObj.value("hasMask").toBool(false));
        m_maskCheck->setChecked(activeLayerObj.value("hasMask").toBool(false) && activeLayerObj.value("maskEnabled").toBool(true));
    }

    m_syncingLayers = false;
    refreshMenuTitles(state.object());
}

void SessionWindow::selectLayerRow(int row) {
    if (m_syncingLayers || row < 0 || !m_layerModel || !m_layersView) return;
    if (row >= m_layerModel->rowCount()) return;
    const QModelIndex idx = m_layerModel->index(row, 0);
    if (!idx.isValid()) return;
    const QString id = idx.data(Qt::UserRole).toString();
    const QByteArray json = QString(R"({"version":1,"action":"selectLayer","layerID":"%1"})").arg(id).toUtf8();
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(json.constData()), json.size()) == 0) refreshImage();
}

void SessionWindow::setOpacityFromSlider(int value) {
    if (m_opacityLabel) m_opacityLabel->setText(QString("%1 %").arg(value));
    if (m_syncingLayers) return;
    const QByteArray json = QString(R"({"version":1,"action":"setOpacity","value":%1})").arg(value / 100.0, 0, 'f', 3).toUtf8();
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(json.constData()), json.size()) == 0) refreshImage();
}

QStringList SessionWindow::blendModes() const {
    return {QStringLiteral("Normal"), QStringLiteral("Multiply"), QStringLiteral("Screen"),
            QStringLiteral("Overlay"), QStringLiteral("Darken"), QStringLiteral("Lighten"),
            QStringLiteral("Color Dodge"), QStringLiteral("Color Burn"), QStringLiteral("Hard Light"),
            QStringLiteral("Soft Light"), QStringLiteral("Difference"), QStringLiteral("Exclusion"),
            QStringLiteral("Hue"), QStringLiteral("Saturation"), QStringLiteral("Color"), QStringLiteral("Luminosity")};
}

void SessionWindow::pickBrushColor() {
    sendCommand({{"action","openColorPicker"},{"enabled",false}}); presentSharedColorPicker();
}

void SessionWindow::setBrushColor(const QColor &color) {
    m_brushColor = color;
    if (m_sessionHandle) sendCommand({{"action", "setPaletteColor"}, {"parameters", QJsonObject{{"red", color.redF()}, {"green", color.greenF()}, {"blue", color.blueF()}}}});
    const QString style = QString("background-color: rgb(%1, %2, %3);").arg(color.red()).arg(color.green()).arg(color.blue());
    m_brushColorButton->setStyleSheet(style);
    m_brushColorButton->setAccessibleName(tr("Foreground color"));
    m_brushColorButton->setText(QString());
}

void SessionWindow::setBrushDiameter(int value) { m_brushDiameter = value; if (m_sessionHandle) sendCommand({{"action", "setBrushSettings"}, {"parameters", QJsonObject{{"diameter", value}}}}); }
void SessionWindow::setBrushHardness(int value) { m_brushHardness = value; if (m_sessionHandle) sendCommand({{"action", "setBrushSettings"}, {"parameters", QJsonObject{{"hardness", value / 100.0}}}}); }
void SessionWindow::setBrushOpacity(int value) { m_brushOpacity = value; if (m_sessionHandle) sendCommand({{"action", "setBrushSettings"}, {"parameters", QJsonObject{{"opacity", value / 100.0}}}}); }

void SessionWindow::setBlendModeFromCombo(int index) {
    if (m_syncingLayers || index < 0) return;
    const QJsonObject command{{"action", "setBlendMode"}, {"kind", blendModes().at(index)}};
    if (sendCommand(command)) refreshImage();
}

void SessionWindow::paintStroke(double x1, double y1, double x2, double y2) {
    const QByteArray begin = QString(
        R"({"version":1,"action":"brushBegin","x":%1,"y":%2,"parameters":{"diameter":%3,"hardness":%4,"opacity":%5,"red":%6,"green":%7,"blue":%8,"erasing":0}})")
        .arg(x1, 0, 'f', 4).arg(y1, 0, 'f', 4)
        .arg(m_brushDiameter).arg(m_brushHardness / 100.0, 0, 'f', 3).arg(m_brushOpacity / 100.0, 0, 'f', 3)
        .arg(m_brushColor.redF(), 0, 'f', 4).arg(m_brushColor.greenF(), 0, 'f', 4).arg(m_brushColor.blueF(), 0, 'f', 4).toUtf8();
    const QByteArray move = QString(R"({"version":1,"action":"brushMove","x":%1,"y":%2})")
        .arg(x2, 0, 'f', 4).arg(y2, 0, 'f', 4).toUtf8();
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(begin.constData()), begin.size()) != 0) return;
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(move.constData()), move.size()) != 0) return;
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(R"({"version":1,"action":"brushEnd"})"), std::strlen(R"({"version":1,"action":"brushEnd"})")) == 0) refreshImage();
}

// Same command mousePressEvent's CloneStamp branch sends for an Option-click.
void SessionWindow::setCloneSource(double x, double y) {
    const QByteArray bytes = QString(R"({"version":1,"action":"cloneSetSource","x":%1,"y":%2})")
        .arg(x, 0, 'f', 4).arg(y, 0, 'f', 4).toUtf8();
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) m_hasCloneSource = true;
}

// Same commands mousePressEvent's CloneStamp branch sends for a stroke, plus the move/end paintStroke sends.
void SessionWindow::cloneStroke(double x1, double y1, double x2, double y2) {
    const QByteArray begin = QString(
        R"({"version":1,"action":"brushBegin","kind":"Clone","x":%1,"y":%2,"parameters":{"diameter":%3,"hardness":%4,"opacity":%5,"aligned":1,"sampleAllLayers":0}})")
        .arg(x1, 0, 'f', 4).arg(y1, 0, 'f', 4)
        .arg(m_brushDiameter).arg(m_brushHardness / 100.0, 0, 'f', 3).arg(m_brushOpacity / 100.0, 0, 'f', 3).toUtf8();
    const QByteArray move = QString(R"({"version":1,"action":"brushMove","x":%1,"y":%2})")
        .arg(x2, 0, 'f', 4).arg(y2, 0, 'f', 4).toUtf8();
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(begin.constData()), begin.size()) != 0) return;
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(move.constData()), move.size()) != 0) return;
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(R"({"version":1,"action":"brushEnd"})"), std::strlen(R"({"version":1,"action":"brushEnd"})")) == 0) refreshImage();
}

// Same commands mousePressEvent's SpotHealing branch sends for a stroke, plus the move/end paintStroke sends.
void SessionWindow::healStroke(double x1, double y1, double x2, double y2) {
    const QByteArray begin = QString(
        R"({"version":1,"action":"brushBegin","x":%1,"y":%2,"parameters":{"diameter":%3,"hardness":%4,"opacity":%5,"red":0,"green":0,"blue":0,"healing":1,"healingMode":0}})")
        .arg(x1, 0, 'f', 4).arg(y1, 0, 'f', 4)
        .arg(m_brushDiameter).arg(m_brushHardness / 100.0, 0, 'f', 3).arg(m_brushOpacity / 100.0, 0, 'f', 3).toUtf8();
    const QByteArray move = QString(R"({"version":1,"action":"brushMove","x":%1,"y":%2})")
        .arg(x2, 0, 'f', 4).arg(y2, 0, 'f', 4).toUtf8();
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(begin.constData()), begin.size()) != 0) return;
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(move.constData()), move.size()) != 0) return;
    if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(R"({"version":1,"action":"brushEnd"})"), std::strlen(R"({"version":1,"action":"brushEnd"})")) == 0) refreshImage();
}

void SessionWindow::setTool(Tool tool) {
    if (m_tool == Tool::Crop && tool != Tool::Crop) {
        cancelCrop();
    }
    m_tool = tool;
    if (m_toolActions.contains(tool) && !m_toolActions[tool]->isChecked()) {
        m_toolActions[tool]->setChecked(true);
    }
    if (m_tool == Tool::Crop && !m_image.isNull() && !m_hasPendingCrop) {
        m_pendingCropRect = QRectF(0, 0, m_image.width(), m_image.height());
        m_hasPendingCrop = true;
    }
    if (m_sessionHandle != 0) {
        const char *toolName = "move";
        switch (tool) {
        case Tool::Move: toolName = "move"; break;
        case Tool::Marquee: toolName = "marquee"; break;
        case Tool::Lasso: toolName = "lasso"; break;
        case Tool::Magic: toolName = "wand"; break;
        case Tool::Crop: toolName = "crop"; break;
        case Tool::Brush: toolName = "brush"; break;
        case Tool::SpotHealing: toolName = "spotHealing"; break;
        case Tool::CloneStamp: toolName = "cloneStamp"; break;
        case Tool::Smear: toolName = "blur"; break;
        case Tool::Gradient: toolName = "gradient"; break;
        case Tool::Shape: toolName = "shape"; break;
        case Tool::Type: toolName = "type"; break;
        case Tool::Eyedropper: toolName = "eyedropper"; break;
        case Tool::Hand: toolName = "hand"; break;
        case Tool::Zoom: toolName = "zoom"; break;
        case Tool::Idle: toolName = "idle"; break;
        }
        QJsonObject cmdObj;
        cmdObj["version"] = 1;
        cmdObj["action"] = "selectTool";
        cmdObj["kind"] = toolName;
        sendCommand(cmdObj);
        if(tool==Tool::Crop && m_hasPendingCrop)
            sendCommand({{"action","setCropRect"},{"x",m_pendingCropRect.x()},{"y",m_pendingCropRect.y()},
                         {"width",qRound(m_pendingCropRect.width())},{"height",qRound(m_pendingCropRect.height())}});
    }
    syncSharedControls();
    updateOptionsBar();
    updateToolRail();
    updateStatusTelemetry();
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::mousePressEvent(QMouseEvent *event) {
    if (event->button() != Qt::LeftButton || m_painting) return;
    const QPointF point = documentPoint(event->position());
    m_dragStart = point;
    if (m_pixelSampler) {
        auto sample = std::move(m_pixelSampler);
        m_pixelSampler = nullptr;
        if (m_canvasWidget) m_canvasWidget->unsetCursor();
        sample(point);
        return;
    }

    switch (m_tool) {
    case Tool::Move: {
        m_transformHandle = -1;
        m_transformDraft = LayerGeometry();
        // Auto Select: pick the topmost visible layer under the cursor unless a
        // handle of the current layer was grabbed.
        const int handle = hitTestTransformHandle(event->position());
        if (handle < 0 && m_autoSelectCheck && m_autoSelectCheck->isChecked()) {
            const bool insideActive = m_activeGeometry.valid && geometryContains(m_activeGeometry, point);
            for (auto it = m_layerGeometries.crbegin(); it != m_layerGeometries.crend(); ++it) {
                if (!it->valid || !it->visible || !geometryContains(*it, point)) continue;
                if (!insideActive || it->id != m_activeGeometry.id) {
                    if (!insideActive && sendCommand({{"action", "selectLayer"}, {"layerID", it->id}})) {
                        refreshImage();
                        refreshLayers();
                    }
                }
                break;
            }
        }
        if (m_activeGeometry.valid) {
            m_transformHandle = handle >= 0 ? handle : (geometryContains(m_activeGeometry, point) ? 9 : -1);
        }
        if (m_transformHandle >= 0) {
            m_transformStart = m_activeGeometry;
            sendCommand({{"action", "transformBegin"}});
        }
        m_painting = true;
        break;
    }
    case Tool::Marquee:
        m_painting = true;
        break;
    case Tool::Crop: {
        m_painting = true;
        if (!m_hasPendingCrop || m_pendingCropRect.isEmpty()) {
            if (!m_image.isNull()) {
                m_pendingCropRect = QRectF(0, 0, m_image.width(), m_image.height());
                m_hasPendingCrop = true;
            }
        }
        m_cropHandle = hitTestCropHandle(event->position());
        if (m_cropHandle < 0) {
            m_dragStart = point;
            m_pendingCropRect = QRectF(point, point);
            m_cropHandle = 4;
            m_hasPendingCrop = true;
        } else {
            m_dragStart = point;
        }
        if (m_canvasWidget) m_canvasWidget->update();
        break;
    }
    case Tool::Lasso: {
        if (!m_polygonalLasso) {
            m_lassoPoints.clear();
            m_lassoPoints.push_back(point);
            m_painting = true;
            break;
        }
        // Polygonal: each click adds a vertex; a double-click or a click on the first vertex closes it.
        const qint64 now = QDateTime::currentMSecsSinceEpoch();
        if (!m_polyActive) {
            m_lassoPoints.clear();
            m_lassoPoints.push_back(point);
            m_polyActive = true;
        } else {
            const QPointF first = documentToCanvasPoint(m_lassoPoints.front());
            const bool onFirst = QLineF(first, event->position()).length() <= 7.0;
            const bool doubleClick = now - m_lastPolyClick < 350 && QLineF(documentToCanvasPoint(m_lassoPoints.back()), event->position()).length() <= 6.0;
            if ((onFirst || doubleClick) && m_lassoPoints.size() >= 3) {
                commitLassoSelection();
                m_polyActive = false;
            } else if (!doubleClick) {
                m_lassoPoints.push_back(point);
            }
        }
        m_lastPolyClick = now;
        break;
    }
    case Tool::Magic: {
        const int contig = m_magicContiguous ? 1 : 0;
        const int sampleAll = m_magicSampleAll ? 1 : 0;
        const QString json = QString(R"({"version":1,"action":"magicWand","x":%1,"y":%2,"kind":"%3","parameters":{"tolerance":%4,"contiguous":%5,"sampleAllLayers":%6}})")
            .arg(point.x(), 0, 'f', 4).arg(point.y(), 0, 'f', 4).arg(m_selectionMode)
            .arg(m_magicTolerance).arg(contig).arg(sampleAll);
        const QByteArray bytes = json.toUtf8();
        if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) {
            refreshImage();
        }
        break;
    }
    case Tool::CloneStamp: {
        if (event->modifiers() & Qt::AltModifier) {
            // Upstream's EditorSession tracks the source and the aligned offset itself (EditorSession.setCloneSource);
            // the host no longer computes or sends an offset.
            const QString json = QString(R"({"version":1,"action":"cloneSetSource","x":%1,"y":%2})")
                .arg(point.x(), 0, 'f', 4).arg(point.y(), 0, 'f', 4);
            const QByteArray bytes = json.toUtf8();
            if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) {
                m_hasCloneSource = true;
                statusBar()->showMessage(tr("Clone Stamp source set to (%1, %2)").arg(qRound(point.x())).arg(qRound(point.y())), 2000);
            }
            return;
        }
        if (!m_hasCloneSource) {
            statusBar()->showMessage(tr("Alt-click where Clone Stamp should copy from first."), 2000);
            return;
        }
        const int aligned = m_cloneAligned ? 1 : 0;
        const int sampleAll = m_cloneSampleAll ? 1 : 0;
        const QString json = QString(
            R"({"version":1,"action":"brushBegin","kind":"Clone","x":%1,"y":%2,"parameters":{"diameter":%3,"hardness":%4,"opacity":%5,"aligned":%6,"sampleAllLayers":%7}})")
            .arg(point.x(), 0, 'f', 4).arg(point.y(), 0, 'f', 4)
            .arg(m_brushDiameter).arg(m_brushHardness / 100.0, 0, 'f', 3).arg(m_brushOpacity / 100.0, 0, 'f', 3)
            .arg(aligned).arg(sampleAll);
        const QByteArray bytes = json.toUtf8();
        if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) {
            m_painting = true;
            refreshImage();
        }
        break;
    }
    case Tool::SpotHealing: {
        int healingMode = 0;
        if (m_spotHealingMode == SpotHealingMode::CreateTexture) healingMode = 1;
        else if (m_spotHealingMode == SpotHealingMode::ProximityMatch) healingMode = 2;
        const QString json = QString(
            R"({"version":1,"action":"brushBegin","x":%1,"y":%2,"parameters":{"diameter":%3,"hardness":%4,"opacity":%5,"red":0,"green":0,"blue":0,"healing":1,"healingMode":%6}})")
            .arg(point.x(), 0, 'f', 4).arg(point.y(), 0, 'f', 4)
            .arg(m_brushDiameter).arg(m_brushHardness / 100.0, 0, 'f', 3).arg(m_brushOpacity / 100.0, 0, 'f', 3)
            .arg(healingMode);
        const QByteArray bytes = json.toUtf8();
        if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) {
            m_painting = true;
            refreshImage();
        }
        break;
    }
    case Tool::Brush: {
        const bool warp = m_brushMode != "Paint";
        const int erasing = (m_brushToolMode == BrushToolMode::Erase) ? 1 : 0;
        const QString json = warp
            ? QString(R"({"version":1,"action":"warpBegin","kind":"%1","x":%2,"y":%3,"parameters":{"diameter":%4,"hardness":%5,"opacity":1}})")
                .arg(m_brushMode).arg(point.x(), 0, 'f', 4).arg(point.y(), 0, 'f', 4).arg(m_brushDiameter).arg(m_brushHardness / 100.0, 0, 'f', 3)
            : QString(R"({"version":1,"action":"brushBegin","x":%1,"y":%2,"parameters":{"diameter":%3,"hardness":%4,"opacity":%5,"red":%6,"green":%7,"blue":%8,"erasing":%9}})")
                .arg(point.x(), 0, 'f', 4).arg(point.y(), 0, 'f', 4)
                .arg(m_brushDiameter).arg(m_brushHardness / 100.0, 0, 'f', 3).arg(m_brushOpacity / 100.0, 0, 'f', 3)
                .arg(m_brushColor.redF(), 0, 'f', 4).arg(m_brushColor.greenF(), 0, 'f', 4).arg(m_brushColor.blueF(), 0, 'f', 4).arg(erasing);
        const QByteArray bytes = json.toUtf8();
        if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) {
            m_painting = true;
            refreshImage();
        }
        break;
    }
    case Tool::Smear: {
        if(sendCommand({{"action","toolPointerBegin"},{"x",point.x()},{"y",point.y()}})) {m_painting=true; refreshImage();}
        break;
    }
    case Tool::Eyedropper: {
        const int px = qFloor(point.x()), py = qFloor(point.y());
        if (!m_image.isNull() && px >= 0 && px < m_image.width() && py >= 0 && py < m_image.height()) {
            const QColor c = m_image.pixelColor(px, py);
            setBrushColor(c);
        }
        break;
    }
    case Tool::Zoom: {
        if (event->modifiers() & Qt::AltModifier) {
            zoomBy(0.8);
        } else {
            zoomBy(1.25);
        }
        break;
    }
    case Tool::Hand: {
        m_painting = true;
        m_panStart = event->position();
        break;
    }
    case Tool::Gradient:
    case Tool::Shape:
        if(sendCommand({{"action","toolPointerBegin"},{"x",point.x()},{"y",point.y()}})) m_painting=true;
        break;
    case Tool::Type:
        if(sendCommand({{"action","toolPointerBegin"},{"x",point.x()},{"y",point.y()}})) showTextEditor();
        break;
    case Tool::Idle:
    default:
        break;
    }
}

void SessionWindow::mouseMoveEvent(QMouseEvent *event) {
    if (!m_painting) return;
    const QPointF point = documentPoint(event->position());
    if (m_tool == Tool::Hand || m_spaceHandActive) {
        const QPointF delta = event->position() - m_panStart;
        m_panStart = event->position();
        m_panOffset += delta;
        if (m_canvasWidget) m_canvasWidget->update();
        return;
    }
    if (m_tool == Tool::Crop && m_hasPendingCrop) {
        QRectF r = m_pendingCropRect;
        const double dx = point.x() - m_dragStart.x();
        const double dy = point.y() - m_dragStart.y();
        m_dragStart = point;
        if (m_cropHandle == 8) {
            r.translate(dx, dy);
        } else {
            switch (m_cropHandle) {
            case 0: r.setTopLeft(r.topLeft() + QPointF(dx, dy)); break;
            case 1: r.setTop(r.top() + dy); break;
            case 2: r.setTopRight(r.topRight() + QPointF(dx, dy)); break;
            case 3: r.setRight(r.right() + dx); break;
            case 4: r.setBottomRight(r.bottomRight() + QPointF(dx, dy)); break;
            case 5: r.setBottom(r.bottom() + dy); break;
            case 6: r.setBottomLeft(r.bottomLeft() + QPointF(dx, dy)); break;
            case 7: r.setLeft(r.left() + dx); break;
            }
        }
        m_pendingCropRect = r.normalized();
        if (m_canvasWidget) m_canvasWidget->update();
        return;
    }
    if(m_tool==Tool::Shape || m_tool==Tool::Gradient || m_tool==Tool::Smear) {
        m_lastPoint=point;
        sendCommand({{"action","toolPointerMove"},{"x",point.x()},{"y",point.y()},
                     {"enabled",bool(event->modifiers()&Qt::ShiftModifier)},{"forward",bool(event->modifiers()&Qt::AltModifier)}});
        refreshImage();
    } else if (m_tool == Tool::Move) {
        if (m_transformHandle >= 0) {
            m_transformDraft = draggedGeometry(point, event->modifiers());
            previewGeometry(m_transformDraft);
        }
    } else if (m_tool == Tool::Lasso) {
        m_lassoPoints.push_back(point);
    } else if (m_tool == Tool::Brush || m_tool == Tool::CloneStamp || m_tool == Tool::SpotHealing || m_tool == Tool::Smear) {
        const QString json = QString(R"({"version":1,"action":"%1","x":%2,"y":%3})")
            .arg(m_brushMode == "Paint" ? "brushMove" : "warpMove")
            .arg(point.x(), 0, 'f', 4).arg(point.y(), 0, 'f', 4);
        const QByteArray bytes = json.toUtf8();
        if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) refreshImage();
    }
}

void SessionWindow::commitLassoSelection() {
    if (m_lassoPoints.size() >= 3) {
        QJsonArray pts;
        for (const QPointF &pt : m_lassoPoints) pts.append(QJsonArray{pt.x(), pt.y()});
        if (sendCommand({{"action", "selectLasso"}, {"points", pts}, {"kind", m_selectionMode}})) refreshImage();
    }
    m_lassoPoints.clear();
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::mouseReleaseEvent(QMouseEvent *event) {
    if (event->button() != Qt::LeftButton || !m_painting) return;
    m_painting = false;
    const QPointF point = documentPoint(event->position());

    switch (m_tool) {
    case Tool::Move: {
        if (m_transformHandle >= 0) {
            if (m_transformDraft.valid) sendCommand({{"action", "transformCommit"}});
            else sendCommand({{"action", "transformCancel"}});
            m_transformHandle = -1;
            m_transformDraft = LayerGeometry();
            refreshImage();
            refreshLayers();
        }
        break;
    }
    case Tool::Marquee: {
        const double x = std::min(m_dragStart.x(), point.x());
        const double y = std::min(m_dragStart.y(), point.y());
        const int w = qRound(std::abs(point.x() - m_dragStart.x()));
        const int h = qRound(std::abs(point.y() - m_dragStart.y()));
        if (w > 0 && h > 0) {
            const char *action = (m_marqueeMode == MarqueeMode::Rectangle) ? "selectRectangle" : "selectEllipse";
            const QString json = QString(R"({"version":1,"action":"%1","x":%2,"y":%3,"width":%4,"height":%5,"kind":"%6"})")
                .arg(action).arg(x, 0, 'f', 2).arg(y, 0, 'f', 2).arg(w).arg(h).arg(m_selectionMode);
            const QByteArray bytes = json.toUtf8();
            if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(bytes.constData()), bytes.size()) == 0) {
                refreshImage();
            }
        }
        break;
    }
    case Tool::Crop: {
        sendCommand({{"action","setCropRect"},{"x",m_pendingCropRect.x()},{"y",m_pendingCropRect.y()},
                     {"width",qRound(m_pendingCropRect.width())},{"height",qRound(m_pendingCropRect.height())}});
        syncSharedControls(); updateOptionsBar();
        m_cropHandle = -1;
        if (m_canvasWidget) m_canvasWidget->update();
        break;
    }
    case Tool::Lasso: {
        m_lassoPoints.push_back(point);
        commitLassoSelection();
        break;
    }
    case Tool::Shape:
    case Tool::Gradient:
    case Tool::Smear:
        sendCommand({{"action","toolPointerMove"},{"x",point.x()},{"y",point.y()},
                     {"enabled",bool(event->modifiers()&Qt::ShiftModifier)},{"forward",bool(event->modifiers()&Qt::AltModifier)}});
        sendCommand({{"action","toolPointerEnd"}}); refreshImage(); refreshLayers(); updateOptionsBar(); break;
    case Tool::Brush:
    case Tool::CloneStamp:
    case Tool::SpotHealing: {
        const char *action = m_brushMode == "Paint" ? "brushEnd" : "warpEnd";
        const QByteArray json = QString(R"({"version":1,"action":"%1"})").arg(action).toUtf8();
        if (compositor_session_command(m_sessionHandle, reinterpret_cast<const uint8_t *>(json.constData()), json.size()) == 0) refreshImage();
        break;
    }
    case Tool::Magic:
    default:
        break;
    }
}

void SessionWindow::tabletEvent(QTabletEvent *event) {
    const QPointF point = documentPoint(event->position());
    if (m_tool == Tool::Brush) {
        switch (event->type()) {
        case QEvent::TabletPress:
            if (m_tabletHandler && m_tabletHandler->handleTabletPress(event, m_sessionHandle, point, m_brushDiameter,
                                                                      m_brushHardness, m_brushOpacity, m_brushColor,
                                                                      m_brushToolMode == BrushToolMode::Erase)) {
                m_painting = true;
                refreshImage();
                event->accept();
                return;
            }
            break;
        case QEvent::TabletMove:
            if (m_tabletHandler && m_tabletHandler->handleTabletMove(event, m_sessionHandle, point, m_painting)) {
                refreshImage();
                event->accept();
                return;
            }
            break;
        case QEvent::TabletRelease:
            if (m_tabletHandler && m_tabletHandler->handleTabletRelease(event, m_sessionHandle, m_painting)) {
                m_painting = false;
                refreshImage();
                event->accept();
                return;
            }
            break;
        default:
            break;
        }
    }
    event->ignore();
}

void SessionWindow::dragEnterEvent(QDragEnterEvent *event) {
    if (event->mimeData()->hasUrls() || event->mimeData()->hasImage()) {
        event->acceptProposedAction();
    }
}

void SessionWindow::dropEvent(QDropEvent *event) {
    if (event->mimeData()->hasUrls()) {
        for (const QUrl &url : event->mimeData()->urls()) {
            if (url.isLocalFile() && importImage(url.toLocalFile(), false)) {
                event->acceptProposedAction();
                return;
            }
        }
    } else if (event->mimeData()->hasImage()) {
        QImage img = qvariant_cast<QImage>(event->mimeData()->imageData());
        if (!img.isNull()) {
            QImage rgba = img.convertToFormat(QImage::Format_RGBA8888);
            std::vector<uint8_t> premul(rgba.width() * rgba.height() * 4);
            const uint8_t *src = rgba.constBits();
            for (size_t i = 0; i < premul.size(); i += 4) {
                uint8_t a = src[i + 3];
                premul[i] = (src[i] * a + 127) / 255;
                premul[i + 1] = (src[i + 1] * a + 127) / 255;
                premul[i + 2] = (src[i + 2] * a + 127) / 255;
                premul[i + 3] = a;
            }
            std::string name = "Dropped Layer";
            if (compositor_session_import_rgba(m_sessionHandle, premul.data(), premul.size(),
                                               rgba.width(), rgba.height(),
                                               reinterpret_cast<const uint8_t*>(name.data()), name.size(), 0) == 0) {
                refreshImage();
                refreshLayers();
                event->acceptProposedAction();
                return;
            }
        }
    }
    event->ignore();
}

// IO milestone: Export flattened canvas as PNG using Qt's QImageWriter
// (replaces macOS CGImageDestination/ImageIO).
// IO milestone: Export flattened canvas as PNG via IImageExporter interface (SOLID)
bool SessionWindow::exportPNG(const QString &path) {
    if (m_sessionHandle == 0 || m_image.isNull()) return false;
    auto exporter = ImageExporterRegistry::instance().exporterForFormat("png");
    return exporter && exporter->exportImage(m_image, path);
}

// IO milestone: Export flattened canvas as JPEG via IImageExporter interface (SOLID)
bool SessionWindow::exportJPEG(const QString &path, int quality) {
    if (m_sessionHandle == 0 || m_image.isNull()) return false;
    auto exporter = ImageExporterRegistry::instance().exporterForFormat("jpeg");
    return exporter && exporter->exportImage(m_image, path, quality);
}

// Parity milestone: Export flattened canvas as TIFF via IImageExporter interface (SOLID)
bool SessionWindow::exportTIFF(const QString &path) {
    if (m_sessionHandle == 0 || m_image.isNull()) return false;
    auto exporter = ImageExporterRegistry::instance().exporterForFormat("tiff");
    return exporter && exporter->exportImage(m_image, path);
}

// Parity milestone: Export flattened canvas as WebP via IImageExporter interface (SOLID)
bool SessionWindow::exportWebP(const QString &path, int quality) {
    if (m_sessionHandle == 0 || m_image.isNull()) return false;
    auto exporter = ImageExporterRegistry::instance().exporterForFormat("webp");
    return exporter && exporter->exportImage(m_image, path, quality);
}

// IO milestone: Import image using Qt's QImageReader
// (replaces macOS CGImageSource/CoreImage). Loads image, converts to
// premultiplied RGBA, and imports via compositor_session_import_rgba.
bool SessionWindow::importImage(const QString &path, bool replacing) {
    if (m_sessionHandle == 0) return false;

    QImageReader reader(path);
    reader.setAutoTransform(true); // Handle EXIF orientation
    QImage image = reader.read();
    if (image.isNull()) return false;

    // Convert to RGBA8888 (straight alpha) then premultiply for the core
    QImage rgba = image.convertToFormat(QImage::Format_RGBA8888);
    if (rgba.isNull()) return false;

    // Premultiply alpha (core expects premultiplied RGBA)
    for (int y = 0; y < rgba.height(); ++y) {
        uint8_t *scan = rgba.scanLine(y);
        for (int x = 0; x < rgba.width(); ++x) {
            uint8_t a = scan[x * 4 + 3];
            if (a != 255) {
                scan[x * 4] = (scan[x * 4] * a + 127) / 255;
                scan[x * 4 + 1] = (scan[x * 4 + 1] * a + 127) / 255;
                scan[x * 4 + 2] = (scan[x * 4 + 2] * a + 127) / 255;
            }
        }
    }

    QByteArray nameBytes = QFileInfo(path).baseName().toUtf8();
    int32_t rc = compositor_session_import_rgba(m_sessionHandle,
                                                rgba.constBits(), rgba.sizeInBytes(),
                                                rgba.width(), rgba.height(),
                                                reinterpret_cast<const uint8_t *>(nameBytes.constData()), nameBytes.size(),
                                                replacing ? 1 : 0);
    if (rc != 0) return false;

    // Update displayed image
    if (replacing) { m_projectPath.clear(); m_recoveredUnsaved = true; m_zoomLevel = 0; m_panOffset = {}; }
    m_hasDocument = true;
    refreshImage();
    refreshLayers();
    return true;
}

// IO milestone: Save project as .compositor package
// (replaces macOS ProjectStore.save with FileWrapper/NSFileCoordinator).
// Writes manifest.json + layer PNGs + mask PNGs into a directory.
bool SessionWindow::saveProject(const QString &path) {
    if (m_sessionHandle == 0 || path.isEmpty()) return false;
    // Never replace an unrelated directory or a symbolic link selected as a save target.
    const QFileInfo target(path);
    if (target.isSymLink() || (target.exists() && (!target.isDir() || !QFileInfo::exists(path + "/manifest.json")))) return false;

    // Export manifest JSON from Swift core
    int64_t manifestSize = compositor_session_export_manifest(m_sessionHandle, nullptr, 0);
    if (manifestSize <= 0) return false;

    std::vector<uint8_t> manifestBytes(static_cast<size_t>(manifestSize));
    int64_t n = compositor_session_export_manifest(m_sessionHandle, manifestBytes.data(), manifestBytes.size());
    if (n != manifestSize) return false;

    QByteArray manifestData(reinterpret_cast<char *>(manifestBytes.data()), manifestSize);
    if (manifestData.size() > 4 * 1024 * 1024) return false;
    QJsonDocument manifestDoc = QJsonDocument::fromJson(manifestData);
    if (manifestDoc.isNull() || !manifestDoc.isObject()) return false;

    const QJsonObject manifest = manifestDoc.object();
    const QString temporaryPath = path + ".tmp-" + QString::number(QCoreApplication::applicationPid());
    QDir temporary(temporaryPath);
    if (temporary.exists() && !temporary.removeRecursively()) return false;
    if (!temporary.mkpath("images")) return false;
    const auto cleanup = qScopeGuard([&] { if (temporary.exists()) temporary.removeRecursively(); });

    QFile manifestFile(temporary.filePath("manifest.json"));
    if (!manifestFile.open(QIODevice::WriteOnly | QIODevice::Truncate)) return false;
    const QByteArray encodedManifest = QJsonDocument(manifest).toJson(QJsonDocument::Indented);
    if (manifestFile.write(encodedManifest) != encodedManifest.size()) return false;
    manifestFile.close();

    for (const QJsonValue &value : manifest.value("layers").toArray()) {
        const QJsonObject layer = value.toObject();
        const QString idString = layer.value("id").toString();
        const QByteArray id = idString.toUtf8();
        if (id.isEmpty()) return false;
        for (const bool isMask : {false, true}) {
            const QString filename = layer.value(isMask ? "maskFile" : "imageFile").toString();
            if (filename.isEmpty()) continue;
            const QString expected = idString + (isMask ? ".mask.png" : ".png");
            if (filename != expected || QFileInfo(filename).fileName() != filename) return false;

            size_t width = 0, height = 0;
            const int64_t size = compositor_session_export_layer(m_sessionHandle,
                reinterpret_cast<const uint8_t *>(id.constData()), static_cast<size_t>(id.size()),
                isMask ? 1 : 0, nullptr, 0, &width, &height);
            if (size <= 0 || width == 0 || height == 0 || static_cast<uint64_t>(size) > 512ULL * 1024 * 1024) return false;
            std::vector<uint8_t> bytes(static_cast<size_t>(size));
            const int64_t copied = compositor_session_export_layer(m_sessionHandle,
                reinterpret_cast<const uint8_t *>(id.constData()), static_cast<size_t>(id.size()),
                isMask ? 1 : 0, bytes.data(), bytes.size(), &width, &height);
            if (copied != size) return false;

            QImage image;
            if (isMask) {
                image = QImage(static_cast<int>(width), static_cast<int>(height), QImage::Format_Grayscale8);
                for (size_t y = 0; y < height; ++y)
                    std::memcpy(image.scanLine(static_cast<int>(y)), bytes.data() + y * width, width);
            } else {
                image = straightRGBA(bytes, static_cast<int>(width), static_cast<int>(height));
            }
            QImageWriter writer(temporary.filePath("images/" + filename), "PNG");
            if (!writer.write(image)) return false;
        }
    }

    QDir destinationInfo = QFileInfo(path).dir();
    const QString backupPath = path + ".old-" + QString::number(QCoreApplication::applicationPid());
    QDir backup(backupPath);
    if (backup.exists() && !backup.removeRecursively()) return false;
    if (QFileInfo::exists(path) && !destinationInfo.rename(path, backupPath)) return false;
    if (!destinationInfo.rename(temporaryPath, path)) {
        if (QFileInfo::exists(backupPath)) destinationInfo.rename(backupPath, path);
        return false;
    }
    if (QFileInfo::exists(backupPath)) backup.removeRecursively();
    if (QFileInfo(path).absoluteFilePath() != QFileInfo(autosaveDirectory() + "/autosave.comp").absoluteFilePath()) {
        m_projectPath = QFileInfo(path).absoluteFilePath();
        m_recoveredUnsaved = false;
        sendCommand({{"action", "markSaved"}});
        clearAutosave();
        updateDocumentTitle();
    }
    return true;
}

// IO milestone: Load project from .compositor package
// (replaces macOS ProjectStore.load with FileWrapper/NSFileCoordinator).
bool SessionWindow::loadProject(const QString &path) {
    if (m_sessionHandle == 0) return false;

    QDir dir(path);
    const QFileInfo packageInfo(path);
    if (!packageInfo.isDir() || packageInfo.isSymLink()) return false;

    const QFileInfo manifestInfo(dir.filePath("manifest.json"));
    if (!manifestInfo.isFile() || manifestInfo.isSymLink() || manifestInfo.size() > 4 * 1024 * 1024) return false;
    QFile manifestFile(manifestInfo.filePath());
    if (!manifestFile.open(QIODevice::ReadOnly)) return false;
    QByteArray manifestData = manifestFile.readAll();
    manifestFile.close();

    QJsonDocument manifestDoc = QJsonDocument::fromJson(manifestData);
    if (manifestDoc.isNull() || !manifestDoc.isObject()) return false;

    const QJsonObject manifest = manifestDoc.object();
    const uint64_t replacement = compositor_session_create();
    if (replacement == 0) return false;
    int32_t rc = compositor_session_import_manifest(replacement,
                                                    reinterpret_cast<const uint8_t *>(manifestData.constData()),
                                                    manifestData.size());
    if (rc != 0) {
        compositor_session_close(replacement);
        return false;
    }
    const QFileInfo imagesInfo(dir.filePath("images"));
    if (!imagesInfo.isDir() || imagesInfo.isSymLink()) {
        compositor_session_close(replacement);
        return false;
    }
    uint64_t imagePixels = 0, maskPixels = 0;

    for (const QJsonValue &value : manifest.value("layers").toArray()) {
        const QJsonObject layer = value.toObject();
        const QString idString = layer.value("id").toString();
        const QByteArray id = idString.toUtf8();
        if (id.isEmpty()) { compositor_session_close(replacement); return false; }
        for (const bool isMask : {false, true}) {
            const QString filename = layer.value(isMask ? "maskFile" : "imageFile").toString();
            if (filename.isEmpty()) continue;
            const QString expected = idString + (isMask ? ".mask.png" : ".png");
            if (filename != expected || QFileInfo(filename).fileName() != filename) {
                compositor_session_close(replacement);
                return false;
            }
            const QFileInfo assetInfo(QDir(imagesInfo.filePath()).filePath(filename));
            if (!assetInfo.isFile() || assetInfo.isSymLink() || assetInfo.size() > 512LL * 1024 * 1024) {
                compositor_session_close(replacement);
                return false;
            }
            QImageReader reader(assetInfo.filePath());
            if (reader.format().toLower() != QByteArray("png")) {
                compositor_session_close(replacement);
                return false;
            }
            const QSize decodedSize = reader.size();
            const uint64_t pixels = decodedSize.isValid()
                ? static_cast<uint64_t>(decodedSize.width()) * static_cast<uint64_t>(decodedSize.height()) : 0;
            uint64_t &usedPixels = isMask ? maskPixels : imagePixels;
            if (!decodedSize.isValid() || decodedSize.width() <= 0 || decodedSize.height() <= 0 ||
                decodedSize.width() > 30'000 || decodedSize.height() > 30'000 ||
                pixels > 100'000'000 || usedPixels > 100'000'000 - pixels) {
                compositor_session_close(replacement);
                return false;
            }
            usedPixels += pixels;
            const QImage decoded = reader.read();
            if (decoded.isNull()) { compositor_session_close(replacement); return false; }
            if (isMask) {
                const QImage gray = decoded.convertToFormat(QImage::Format_Grayscale8);
                std::vector<uint8_t> pixels(static_cast<size_t>(gray.width()) * gray.height());
                for (int y = 0; y < gray.height(); ++y)
                    std::memcpy(pixels.data() + static_cast<size_t>(y) * gray.width(), gray.constScanLine(y), gray.width());
                rc = compositor_session_import_layer(replacement,
                    reinterpret_cast<const uint8_t *>(id.constData()), static_cast<size_t>(id.size()), 1,
                    pixels.data(), pixels.size(), gray.width(), gray.height());
            } else {
                const QImage rgba = decoded.convertToFormat(QImage::Format_RGBA8888);
                std::vector<uint8_t> pixels(static_cast<size_t>(rgba.sizeInBytes()));
                for (int y = 0; y < rgba.height(); ++y) {
                    const uint8_t *source = rgba.constScanLine(y);
                    uint8_t *target = pixels.data() + static_cast<size_t>(y) * rgba.width() * 4;
                    for (int x = 0; x < rgba.width(); ++x) {
                        const uint8_t alpha = source[x * 4 + 3];
                        target[x * 4] = static_cast<uint8_t>((source[x * 4] * alpha + 127) / 255);
                        target[x * 4 + 1] = static_cast<uint8_t>((source[x * 4 + 1] * alpha + 127) / 255);
                        target[x * 4 + 2] = static_cast<uint8_t>((source[x * 4 + 2] * alpha + 127) / 255);
                        target[x * 4 + 3] = alpha;
                    }
                }
                rc = compositor_session_import_layer(replacement,
                    reinterpret_cast<const uint8_t *>(id.constData()), static_cast<size_t>(id.size()), 0,
                    pixels.data(), pixels.size(), rgba.width(), rgba.height());
            }
            if (rc != 0) { compositor_session_close(replacement); return false; }
        }
    }

    // Render the loaded document
    int width = manifest["width"].toInt();
    int height = manifest["height"].toInt();
    if (width <= 0 || height <= 0) { compositor_session_close(replacement); return false; }
    QImage rendered = renderToQImage(replacement, width, height);
    if (rendered.isNull()) { compositor_session_close(replacement); return false; }
    compositor_session_close(m_sessionHandle);
    m_sessionHandle = replacement;
    m_image = rendered;
    m_projectPath = QFileInfo(path).absoluteFilePath();
    m_recoveredUnsaved = false;
    m_hasDocument = true;
    m_zoomLevel = 0; m_panOffset = {};
    m_hasPendingCrop = false; m_hasCloneSource = false; m_spaceHandActive = false;
    setTool(Tool::Move);
    syncSharedControls(); updateOptionsBar(); updateToolRail(); applyShortcuts();
    refreshImage(); refreshLayers();
    return true;
}

void SessionWindow::deleteSelectedLayers() {
    if (m_sessionHandle == 0 || !m_layersView || !m_layerModel) return;
    const QModelIndexList selectedRows = m_layersView->selectionModel()->selectedRows();
    if (selectedRows.isEmpty()) {
        sendCommand({{"action", "deleteLayer"}});
        refreshImage();
        refreshLayers();
        return;
    }

    QStringList ids;
    for (const QModelIndex &idx : selectedRows) {
        const QString id = idx.data(Qt::UserRole).toString();
        if (!id.isEmpty()) {
            ids.append(id);
        }
    }

    for (const QString &id : ids) {
        sendCommand({{"action", "selectLayer"}, {"layerID", id}});
        sendCommand({{"action", "deleteLayer"}});
    }
    refreshImage();
    refreshLayers();
}

QString SessionWindow::autosaveDirectory() const {
    return m_platform.storage->appDataDirectory() + "/recovery/" + m_recoveryID;
}

bool SessionWindow::hasAutosaveRecovery() const {
    const QString recoveryDir = autosaveDirectory();
    const QString manifestPath = recoveryDir + "/autosave.comp/manifest.json";
    return QFileInfo::exists(manifestPath);
}

bool SessionWindow::performAutosave() {
    if (m_sessionHandle == 0) return false;
    const auto state = sessionState();
    if (state.value("busy").toBool()) return false;
    if (!hasUnsavedChanges()) { clearAutosave(); return false; }

    const QString recoveryDir = autosaveDirectory();
    QDir dir(recoveryDir);
    if (!dir.exists() && !dir.mkpath(".")) return false;

    if (!m_recoveryLock) {
        auto lock = std::make_unique<QLockFile>(recoveryDir + "/owner.lock");
        lock->setStaleLockTime(0);
        if (!lock->tryLock()) return false;
        m_recoveryLock = std::move(lock);
    }
    const QString packagePath = recoveryDir + "/autosave.comp";
    if (!saveProject(packagePath)) return false;

    QJsonObject infoObj{
        {"savedAt", QDateTime::currentDateTimeUtc().toString(Qt::ISODate)},
        {"width", state.value("width").toInt()},
        {"height", state.value("height").toInt()}
    };
    QFile infoFile(recoveryDir + "/autosave.info");
    if (infoFile.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        infoFile.write(QJsonDocument(infoObj).toJson(QJsonDocument::Indented));
        infoFile.close();
    }
    return true;
}

bool SessionWindow::recoverAutosave() {
    if (!hasAutosaveRecovery()) return false;
    const QString packagePath = autosaveDirectory() + "/autosave.comp";
    if (!loadProject(packagePath)) return false;
    m_projectPath.clear(); m_recoveredUnsaved = true; updateDocumentTitle();
    return true;
}

void SessionWindow::clearAutosave() {
    const QString recoveryDir = autosaveDirectory();
    QDir package(recoveryDir + "/autosave.comp");
    if (package.exists()) {
        package.removeRecursively();
    }
    QFile::remove(recoveryDir + "/autosave.info");
    m_recoveryLock.reset();
    QDir().rmdir(recoveryDir);
    if (!m_recoveredDirectory.isEmpty()) {
        QDir(m_recoveredDirectory + "/autosave.comp").removeRecursively();
        QFile::remove(m_recoveredDirectory + "/autosave.info");
        m_recoveredLock.reset(); QDir().rmdir(m_recoveredDirectory);
        m_recoveredDirectory.clear();
    }
}

bool SessionWindow::hasUnsavedChanges() const {
    return m_recoveredUnsaved || sessionState().value("modified").toBool();
}

void SessionWindow::updateDocumentTitle() {
    const QString name = m_projectPath.isEmpty() ? tr("Untitled") : QFileInfo(m_projectPath).fileName();
    const bool dirty = hasUnsavedChanges();
    setWindowTitle(name + "[*] — Compositor");
    setWindowModified(dirty);
    if (m_documentTabBar && m_documentTabBar->count()) {
        m_documentTabBar->setTabText(0, name + (dirty ? QStringLiteral(" •") : QString()));
        m_documentTabBar->setTabToolTip(0, m_projectPath);
    }
}

bool SessionWindow::saveCurrentProject(bool saveAs) {
    QString path = saveAs ? QString() : m_projectPath;
    if (path.isEmpty()) {
        path = m_platform.files->chooseProjectSavePath();
        if (path.isEmpty()) return false;
        if (QFileInfo(path).suffix().isEmpty()) path += ".comp";
    }
    if (saveProject(path)) return true;
    m_platform.notifier->warn(tr("Save failed"), tr("Could not save the project. Your document remains open with unsaved changes."));
    return false;
}

bool SessionWindow::confirmDocumentReplacement() {
    if (sessionState().value("busy").toBool()) return false;
    if (!hasUnsavedChanges()) return true;
    QMessageBox prompt(QMessageBox::Warning, tr("Unsaved changes"),
        tr("Save changes to %1 before closing it?").arg(m_projectPath.isEmpty() ? tr("Untitled") : QFileInfo(m_projectPath).fileName()),
        QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, this);
    prompt.setObjectName("document.unsaved"); prompt.setDefaultButton(QMessageBox::Save);
    prompt.setEscapeButton(QMessageBox::Cancel);
    const int result = prompt.exec();
    return result == QMessageBox::Discard || (result == QMessageBox::Save && saveCurrentProject());
}

void SessionWindow::offerRecovery() {
    const QString root = m_platform.storage->appDataDirectory() + "/recovery";
    QStringList directories;
    for (const QString &entry : QDir(root).entryList(QDir::Dirs | QDir::NoDotAndDotDot, QDir::Time))
        directories << root + "/" + entry;
    directories << root; // Packages from older versions.
    for (const QString &directory : directories) {
        if (!QFileInfo::exists(directory + "/autosave.comp/manifest.json")) continue;
        auto lock = std::make_unique<QLockFile>(directory + "/owner.lock");
        lock->setStaleLockTime(0);
        if (!lock->tryLock()) continue; // Another editor still owns this document.
        QMessageBox prompt(QMessageBox::Question, tr("Recover unsaved work"),
            tr("An unsaved project was found. Recover it now? Choosing No keeps it for later."),
            QMessageBox::Yes | QMessageBox::No, this);
        prompt.setObjectName("document.recovery"); prompt.setDefaultButton(QMessageBox::Yes);
        if (prompt.exec() != QMessageBox::Yes) return;
        if (loadProject(directory + "/autosave.comp")) {
            m_recoveredDirectory = directory; m_recoveredLock = std::move(lock);
            m_projectPath.clear(); m_recoveredUnsaved = true; updateDocumentTitle();
        } else m_platform.notifier->warn(tr("Recovery failed"), tr("The recovery copy could not be opened and has been kept."));
        return;
    }
}

void SessionWindow::closeEvent(QCloseEvent *event) {
    if (!confirmDocumentReplacement()) { event->ignore(); return; }
    clearAutosave();
    QMainWindow::closeEvent(event);
}

// ---- Move / Transform tool ---------------------------------------------------


void SessionWindow::syncTransformFields() {
    if (!m_xSpin) return;
    const bool enabled = m_activeGeometry.valid;
    for (QSpinBox *spin : {m_xSpin, m_ySpin, m_wSpin, m_hSpin, m_angleSpin}) {
        const QSignalBlocker blocker(spin);
        spin->setEnabled(enabled);
    }
    if (!enabled) return;
    const QSignalBlocker bx(m_xSpin), by(m_ySpin), bw(m_wSpin), bh(m_hSpin), ba(m_angleSpin);
    m_xSpin->setValue(qRound(m_activeGeometry.x));
    m_ySpin->setValue(qRound(m_activeGeometry.y));
    m_wSpin->setValue(qRound(m_activeGeometry.w));
    m_hSpin->setValue(qRound(m_activeGeometry.h));
    m_angleSpin->setValue(qRound(m_activeGeometry.rotation));
}

void SessionWindow::applyTransformFields(int changedField) {
    if (m_syncingLayers || !m_activeGeometry.valid || m_sessionHandle == 0) return;
    LayerGeometry next = m_activeGeometry;
    next.x = m_xSpin->value();
    next.y = m_ySpin->value();
    next.rotation = m_angleSpin->value();
    if (changedField == 2 || changedField == 3) {
        const double aspect = m_activeGeometry.w / std::max(1.0, m_activeGeometry.h);
        if (m_linkCheck && m_linkCheck->isChecked()) {
            if (changedField == 2) { next.w = m_wSpin->value(); next.h = std::max(1.0, std::round(next.w / aspect)); }
            else { next.h = m_hSpin->value(); next.w = std::max(1.0, std::round(next.h * aspect)); }
        } else {
            next.w = m_wSpin->value();
            next.h = m_hSpin->value();
        }
    }
    const QJsonObject command{{"action", "transform"}, {"parameters", QJsonObject{
        {"x", next.x}, {"y", next.y}, {"width", next.w}, {"height", next.h}, {"rotation", next.rotation}}}};
    if (sendCommand(command)) { refreshImage(); refreshLayers(); }
}

int SessionWindow::hitTestTransformHandle(const QPointF &canvasPoint) const {
    if (!m_activeGeometry.valid) return -1;
    const LayerGeometry &g = m_activeGeometry;
    const double reach = 8.0;
    const QPointF top = documentToCanvasPoint(geometryPoint(g, QPointF(0.5, 0)));
    const QPointF center = documentToCanvasPoint(geometryPoint(g, QPointF(0.5, 0.5)));
    QPointF outward = top - center;
    const double length = std::hypot(outward.x(), outward.y());
    if (length > 0) outward /= length;
    if (QLineF(canvasPoint, top + outward * 26.0).length() <= reach) return 8;
    for (int i = 0; i < 8; ++i) {
        if (QLineF(canvasPoint, documentToCanvasPoint(geometryPoint(g, kHandleUnits[i]))).length() <= reach) return i;
    }
    return -1;
}

void SessionWindow::drawTransformControls(QPainter &p) const {
    if (m_tool != Tool::Move || !m_activeGeometry.valid || !m_showControlsCheck || !m_showControlsCheck->isChecked()) return;
    const LayerGeometry &g = (m_transformHandle >= 0 && m_transformDraft.valid) ? m_transformDraft : m_activeGeometry;
    p.save();
    p.setRenderHint(QPainter::Antialiasing, true);
    QPolygonF outline;
    for (const QPointF &corner : {QPointF(0, 0), QPointF(1, 0), QPointF(1, 1), QPointF(0, 1)}) {
        outline << documentToCanvasPoint(geometryPoint(g, corner));
    }
    p.setPen(QPen(QColor(0xf2, 0xf2, 0xf5), 1));
    p.setBrush(Qt::NoBrush);
    p.drawPolygon(outline);
    const QPointF top = documentToCanvasPoint(geometryPoint(g, QPointF(0.5, 0)));
    const QPointF center = documentToCanvasPoint(geometryPoint(g, QPointF(0.5, 0.5)));
    QPointF outward = top - center;
    const double length = std::hypot(outward.x(), outward.y());
    if (length > 0) outward /= length;
    const QPointF rotateHandle = top + outward * 26.0;
    p.drawLine(top, rotateHandle);
    p.setBrush(QColor(0xf2, 0xf2, 0xf5));
    p.drawEllipse(rotateHandle, 3.5, 3.5);
    p.setBrush(QColor(0xff, 0xff, 0xff));
    p.setPen(QPen(QColor(0x50, 0x50, 0x58), 1));
    for (const QPointF &unit : kHandleUnits) {
        const QPointF c = documentToCanvasPoint(geometryPoint(g, unit));
        p.drawRect(QRectF(c.x() - 4, c.y() - 4, 8, 8));
    }
    p.restore();
}

void SessionWindow::previewGeometry(const LayerGeometry &g) {
    const QJsonObject command{{"action", "transformPreview"}, {"parameters", QJsonObject{
        {"x", g.x}, {"y", g.y}, {"width", g.w}, {"height", g.h}, {"rotation", g.rotation}}}};
    if (sendCommand(command)) refreshImage();
}

SessionWindow::LayerGeometry SessionWindow::draggedGeometry(const QPointF &point, Qt::KeyboardModifiers modifiers) const {
    LayerGeometry g = m_transformStart;
    const double r = g.rotation * kPi / 180.0;
    if (m_transformHandle == 9) {
        g.x += point.x() - m_dragStart.x();
        g.y += point.y() - m_dragStart.y();
        return g;
    }
    const QPointF c0(g.x + g.w / 2, g.y + g.h / 2);
    if (m_transformHandle == 8) {
        double degrees = std::atan2(point.y() - c0.y(), point.x() - c0.x()) * 180.0 / kPi + 90.0;
        if (modifiers & Qt::ShiftModifier) degrees = std::round(degrees / 15.0) * 15.0;
        while (degrees > 180.0) degrees -= 360.0;
        while (degrees <= -180.0) degrees += 360.0;
        g.rotation = degrees;
        return g;
    }
    // Resize in the layer's own (rotated) frame around the opposite edge / corner.
    const QPointF unit = kHandleUnits[m_transformHandle];
    const double dx = point.x() - c0.x(), dy = point.y() - c0.y();
    const QPointF local(dx * std::cos(r) + dy * std::sin(r), -dx * std::sin(r) + dy * std::cos(r));
    const bool moveX = unit.x() != 0.5, moveY = unit.y() != 0.5;
    const double signX = unit.x() > 0.5 ? 1.0 : -1.0, signY = unit.y() > 0.5 ? 1.0 : -1.0;
    const double anchorX = moveX ? -signX * g.w / 2 : 0, anchorY = moveY ? -signY * g.h / 2 : 0;
    double newW = moveX ? std::max(1.0, signX * (local.x() - anchorX)) : g.w;
    double newH = moveY ? std::max(1.0, signY * (local.y() - anchorY)) : g.h;
    const bool corner = moveX && moveY;
    if (corner && ((m_linkCheck && m_linkCheck->isChecked()) != bool(modifiers & Qt::ShiftModifier))) {
        const double scale = std::max(newW / g.w, newH / g.h);
        newW = std::max(1.0, g.w * scale);
        newH = std::max(1.0, g.h * scale);
    }
    const double cx = moveX ? anchorX + signX * newW / 2 : 0;
    const double cy = moveY ? anchorY + signY * newH / 2 : 0;
    const QPointF center(c0.x() + cx * std::cos(r) - cy * std::sin(r), c0.y() + cx * std::sin(r) + cy * std::cos(r));
    g.w = std::round(newW);
    g.h = std::round(newH);
    g.x = center.x() - g.w / 2;
    g.y = center.y() - g.h / 2;
    return g;
}

int SessionWindow::hitTestCropHandle(const QPointF &canvasPoint) const {
    if (!m_hasPendingCrop || m_pendingCropRect.isEmpty() || m_image.isNull()) return -1;
    const double reach = 10.0;
    const QRectF cRect(documentToCanvasPoint(m_pendingCropRect.topLeft()),
                       documentToCanvasPoint(m_pendingCropRect.bottomRight()));
    const QRectF norm = cRect.normalized();
    const QPointF handles[8] = {
        norm.topLeft(),
        QPointF(norm.center().x(), norm.top()),
        norm.topRight(),
        QPointF(norm.right(), norm.center().y()),
        norm.bottomRight(),
        QPointF(norm.center().x(), norm.bottom()),
        norm.bottomLeft(),
        QPointF(norm.left(), norm.center().y())
    };
    for (int i = 0; i < 8; ++i) {
        if (QLineF(canvasPoint, handles[i]).length() <= reach) return i;
    }
    if (norm.contains(canvasPoint)) return 8; // Body move
    return -1;
}

void SessionWindow::drawCropOverlay(QPainter &p) const {
    if (m_tool != Tool::Crop || !m_hasPendingCrop || m_pendingCropRect.isEmpty() || m_image.isNull()) return;
    const QRectF target = canvasTargetRect();
    const QRectF cRect(documentToCanvasPoint(m_pendingCropRect.topLeft()),
                       documentToCanvasPoint(m_pendingCropRect.bottomRight()));
    const QRectF norm = cRect.normalized().intersected(target);
    if (norm.isEmpty()) return;

    p.save();
    p.setRenderHint(QPainter::Antialiasing, true);

    // Dim outer canvas area
    QPainterPath dimPath;
    dimPath.addRect(target);
    dimPath.addRect(norm);
    p.fillPath(dimPath, QColor(0, 0, 0, 140));

    // Rule-of-thirds grid
    p.setPen(QPen(QColor(255, 255, 255, 80), 1, Qt::SolidLine));
    const double w3 = norm.width() / 3.0;
    const double h3 = norm.height() / 3.0;
    p.drawLine(QPointF(norm.left() + w3, norm.top()), QPointF(norm.left() + w3, norm.bottom()));
    p.drawLine(QPointF(norm.left() + w3 * 2, norm.top()), QPointF(norm.left() + w3 * 2, norm.bottom()));
    p.drawLine(QPointF(norm.left(), norm.top() + h3), QPointF(norm.right(), norm.top() + h3));
    p.drawLine(QPointF(norm.left(), norm.top() + h3 * 2), QPointF(norm.right(), norm.top() + h3 * 2));

    // White border
    p.setPen(QPen(Qt::white, 1));
    p.setBrush(Qt::NoBrush);
    p.drawRect(norm);

    // Corner / edge handles
    p.setPen(QPen(QColor(0x30, 0x30, 0x30), 1));
    p.setBrush(Qt::white);
    const QPointF handles[8] = {
        norm.topLeft(),
        QPointF(norm.center().x(), norm.top()),
        norm.topRight(),
        QPointF(norm.right(), norm.center().y()),
        norm.bottomRight(),
        QPointF(norm.center().x(), norm.bottom()),
        norm.bottomLeft(),
        QPointF(norm.left(), norm.center().y())
    };
    for (const QPointF &pt : handles) {
        p.drawRect(QRectF(pt.x() - 4, pt.y() - 4, 8, 8));
    }
    p.restore();
}

void SessionWindow::applyDarkTheme() {
    QPalette darkPalette;
    darkPalette.setColor(QPalette::Window, QColor(0x1e, 0x1e, 0x20));
    darkPalette.setColor(QPalette::WindowText, QColor(0xf5, 0xf5, 0xf7));
    darkPalette.setColor(QPalette::Base, QColor(0x24, 0x24, 0x27));
    darkPalette.setColor(QPalette::AlternateBase, QColor(0x2a, 0x2a, 0x2d));
    darkPalette.setColor(QPalette::ToolTipBase, QColor(0x1e, 0x1e, 0x20));
    darkPalette.setColor(QPalette::ToolTipText, QColor(0xf5, 0xf5, 0xf7));
    darkPalette.setColor(QPalette::Text, QColor(0xf5, 0xf5, 0xf7));
    darkPalette.setColor(QPalette::Button, QColor(0x2a, 0x2a, 0x2d));
    darkPalette.setColor(QPalette::ButtonText, QColor(0xf5, 0xf5, 0xf7));
    darkPalette.setColor(QPalette::BrightText, Qt::white);
    darkPalette.setColor(QPalette::Link, QColor(0x00, 0x7a, 0xff));
    darkPalette.setColor(QPalette::Highlight, QColor(0x00, 0x7a, 0xff));
    darkPalette.setColor(QPalette::HighlightedText, Qt::white);
    darkPalette.setColor(QPalette::Disabled, QPalette::Text, QColor(0x6e, 0x6e, 0x73));
    darkPalette.setColor(QPalette::Disabled, QPalette::ButtonText, QColor(0x6e, 0x6e, 0x73));
    darkPalette.setColor(QPalette::Disabled, QPalette::WindowText, QColor(0x6e, 0x6e, 0x73));
    if (qApp) {
        qApp->setPalette(darkPalette);
    }

    const QString qss = QString::fromUtf8(R"(
        QMainWindow {
            background-color: #1e1e1e;
            color: #f5f5f7;
        }
        QWidget {
            color: #f5f5f7;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            font-size: 12px;
        }
        QMenuBar {
            background-color: #1e1e20;
            color: #f5f5f7;
            border-bottom: 1px solid #141416;
            padding: 2px 6px;
        }
        QMenuBar::item {
            background: transparent;
            color: #f5f5f7;
            padding: 4px 8px;
            border-radius: 4px;
        }
        QMenuBar::item:selected {
            background-color: #2c2c30;
            color: #ffffff;
        }
        QMenu {
            background-color: #242427;
            color: #f5f5f7;
            border: 1px solid #38383c;
            border-radius: 6px;
            padding: 4px;
        }
        QMenu::item {
            padding: 5px 28px 5px 24px;
            border-radius: 4px;
            color: #f5f5f7;
            background-color: transparent;
        }
        QMenu::item:selected {
            background-color: #007aff;
            color: #ffffff;
        }
        QMenu::item:disabled {
            color: #6e6e73;
            background-color: transparent;
        }
        QMenu::separator {
            height: 1px;
            background-color: #38383c;
            margin: 4px 8px;
        }
        QMenu::indicator {
            width: 14px;
            height: 14px;
            left: 6px;
        }
        QMenu::indicator:checked {
            background-color: #007aff;
            border: 1px solid #007aff;
            border-radius: 3px;
        }
        QMenu::right-arrow {
            margin: 5px;
        }
        QToolBar {
            background-color: #1e1e20;
            border: none;
            spacing: 4px;
            padding: 2px;
        }
        QToolBar#toolbar.tools {
            background-color: #1e1e20;
            border-right: 1px solid #141416;
            spacing: 10px;
            padding-top: 16px;
            padding-bottom: 12px;
            width: 56px;
            min-width: 56px;
            max-width: 56px;
        }
        QToolBar#toolbar.tools QToolButton {
            background: transparent;
            color: #f5f5f7;
            border: 1px solid transparent;
            border-radius: 7px;
            padding: 0px;
            margin: 0px auto;
            width: 36px;
            height: 36px;
            min-width: 36px;
            min-height: 36px;
            max-width: 36px;
            max-height: 36px;
        }
        QToolBar#toolbar.tools QToolButton:hover {
            background-color: rgba(255, 255, 255, 0.08);
            color: #ffffff;
            border: 1px solid rgba(255, 255, 255, 0.10);
        }
        QToolBar#toolbar.tools QToolButton:checked {
            background-color: rgba(255, 255, 255, 0.15);
            color: #ffffff;
            border: 1px solid rgba(255, 255, 255, 0.18);
            border-radius: 7px;
        }
        QStatusBar {
            background-color: #1e1e20;
            color: #f5f5f7;
            min-height: 30px;
            max-height: 30px;
            padding: 0px 18px;
            border-top: 1px solid #141416;
        }
        QStatusBar QLabel {
            font-size: 11px;
            color: #e5e5e7;
            padding: 0px;
        }
        QToolBar::separator {
            background-color: #2d2d30;
            height: 1px;
            margin: 4px 4px;
        }
        QDockWidget {
            background-color: #1e1e20;
            color: #f5f5f7;
            border: none;
        }
        QDockWidget::title {
            background-color: #1e1e20;
            color: #ffffff;
            padding: 8px 12px;
            border-bottom: 1px solid #141416;
            font-weight: 600;
            font-size: 12px;
        }
        QDockWidget > QWidget {
            background-color: #1e1e20;
            border-left: 1px solid #141416;
        }
        QTreeView {
            background-color: #1a1a1c;
            alternate-background-color: #202023;
            color: #ffffff;
            border: none;
            outline: 0;
            selection-background-color: #3a3b3f;
            selection-color: #ffffff;
            show-decoration-selected: 1;
        }
        QHeaderView::section {
            background-color: #1e1e20;
            color: #d0d0d5;
            padding: 4px 8px;
            border: none;
            border-bottom: 1px solid #28282c;
            font-size: 11px;
            font-weight: 600;
        }
        QSlider::groove:horizontal {
            height: 4px;
            background: #333336;
            border-radius: 2px;
        }
        QSlider::sub-page:horizontal {
            background: #007aff;
            border-radius: 2px;
        }
        QSlider::handle:horizontal {
            background: #ffffff;
            border: 1px solid #b0b0b5;
            width: 12px;
            height: 12px;
            margin: -4px 0;
            border-radius: 6px;
        }
        QSlider::handle:horizontal:hover {
            background: #f0f0f5;
        }
        QComboBox {
            background-color: #28282b;
            color: #ffffff;
            border: 1px solid #444448;
            border-radius: 4px;
            padding: 3px 8px;
            min-height: 18px;
            font-size: 11px;
        }
        QComboBox:hover {
            border-color: #5a5a60;
        }
        QComboBox::drop-down {
            subcontrol-origin: padding;
            subcontrol-position: top right;
            width: 18px;
            border-left-width: 0px;
        }
        QComboBox QAbstractItemView {
            background-color: #242427;
            color: #ffffff;
            border: 1px solid #38383c;
            selection-background-color: #007aff;
            selection-color: #ffffff;
            padding: 4px;
        }
        QCheckBox {
            color: #f5f5f7;
            font-size: 11px;
            spacing: 6px;
        }
        QCheckBox::indicator {
            width: 14px;
            height: 14px;
            border: 1px solid #4a4a50;
            border-radius: 3px;
            background-color: #28282b;
        }
        QCheckBox::indicator:hover {
            border-color: #65656d;
        }
        QCheckBox::indicator:checked {
            background-color: #007aff;
            border-color: #007aff;
        }
        QSpinBox, QDoubleSpinBox, QLineEdit {
            background-color: #28282b;
            color: #ffffff;
            border: 1px solid #444448;
            border-radius: 4px;
            padding: 2px 4px;
            font-size: 11px;
        }
        QSpinBox:focus, QDoubleSpinBox:focus, QLineEdit:focus {
            border: 1px solid #007aff;
        }
        QPushButton {
            background-color: #2a2a2d;
            color: #ffffff;
            border: 1px solid #38383c;
            border-radius: 4px;
            padding: 4px 10px;
            font-size: 11px;
            font-weight: 500;
        }
        QPushButton:hover {
            background-color: #35353a;
            color: #ffffff;
            border-color: #55555c;
        }
        QPushButton:pressed {
            background-color: #1f1f22;
        }
    )");
    if (qApp) {
        qApp->setStyleSheet(qss);
    }
    setStyleSheet(qss);
}

void SessionWindow::setupHeaderBar() {
    m_headerToolBar = addToolBar(tr("Header"));
    m_headerToolBar->setObjectName("toolbar.header");
    m_headerToolBar->setMovable(false);
    m_headerToolBar->setFixedHeight(36);
    m_headerToolBar->setStyleSheet("QToolBar { background: #1e1e20; border-bottom: 1px solid #141416; spacing: 6px; padding: 2px 8px; }");

    auto *btnNew = new QPushButton("+", m_headerToolBar);
    btnNew->setObjectName("newCanvasToolbar");
    btnNew->setToolTip(tr("New canvas (Ctrl+N)"));
    btnNew->setFixedSize(26, 26);
    btnNew->setStyleSheet(
        "QPushButton { background: #2a2a2d; color: #d0d0d0; border: 1px solid #3a3a3d; border-radius: 4px; font-size: 15px; font-weight: bold; } "
        "QPushButton:hover { background: #353539; color: #ffffff; } "
        "QPushButton:pressed { background: #202022; }"
    );
    connect(btnNew, &QPushButton::clicked, this, [this] {
        if (auto *action = findChild<QAction *>("file.new")) action->trigger();
    });
    m_headerToolBar->addWidget(btnNew);

    m_documentTabBar = new QTabBar(m_headerToolBar);
    m_documentTabBar->setObjectName("header.documentTabs");
    m_documentTabBar->setDrawBase(false);
    m_documentTabBar->setExpanding(false);
    m_documentTabBar->setTabsClosable(true);
    m_documentTabBar->addTab(tr("Untitled 1"));
    m_documentTabBar->setStyleSheet(
        "QTabBar::tab { background: #252528; color: #9a9a9f; border: 1px solid #333336; border-radius: 5px; padding: 3px 12px; margin-right: 4px; font-size: 12px; } "
        "QTabBar::tab:selected { background: #38393e; color: #ffffff; border: 1px solid #4a4b52; } "
        "QTabBar::tab:hover:!selected { background: #2e2f33; color: #d0d0d5; } "
        "QTabBar::close-button { subcontrol-position: right; margin-left: 4px; } "
        "QTabBar::close-button:hover { background: #55555a; border-radius: 2px; }"
    );
    connect(m_documentTabBar, &QTabBar::tabCloseRequested, this, [this](int) { close(); });
    m_headerToolBar->addWidget(m_documentTabBar);

    auto *spacer = new QWidget(m_headerToolBar);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    m_headerToolBar->addWidget(spacer);

    auto makeZoomBtn = [this](const QString &text, const QString &objName, const QString &tooltip) {
        auto *btn = new QPushButton(text, m_headerToolBar);
        btn->setObjectName(objName);
        btn->setToolTip(tooltip);
        btn->setFixedHeight(24);
        btn->setStyleSheet(
            "QPushButton { background: #2a2a2d; color: #c8c8cd; border: 1px solid #38383c; border-radius: 4px; padding: 2px 8px; font-size: 11px; font-weight: 500; } "
            "QPushButton:hover { background: #35353a; color: #ffffff; border-color: #4a4a50; } "
            "QPushButton:pressed { background: #202022; }"
        );
        return btn;
    };

    auto *btnFit = makeZoomBtn(tr("Fit"), "fitCanvas", tr("Fit canvas in window (Ctrl+0)"));
    connect(btnFit, &QPushButton::clicked, this, &SessionWindow::fitCanvas);
    m_headerToolBar->addWidget(btnFit);

    auto *btn100 = makeZoomBtn(tr("100%"), "actualPixels", tr("Actual pixels (Ctrl+1)"));
    connect(btn100, &QPushButton::clicked, this, &SessionWindow::actualPixels);
    m_headerToolBar->addWidget(btn100);

    auto *btnZoomOut = makeZoomBtn(QString::fromUtf8("−"), "zoomOut", tr("Zoom out (Ctrl+−)"));
    connect(btnZoomOut, &QPushButton::clicked, this, [this] { zoomBy(1.0 / 1.25); });
    m_headerToolBar->addWidget(btnZoomOut);

    auto *btnZoomIn = makeZoomBtn("+", "zoomIn", tr("Zoom in (Ctrl++)"));
    connect(btnZoomIn, &QPushButton::clicked, this, [this] { zoomBy(1.25); });
    m_headerToolBar->addWidget(btnZoomIn);
}

void SessionWindow::setupOptionsBar() {
    addToolBarBreak(Qt::TopToolBarArea);
    m_optionsToolBar = addToolBar(tr("Options"));
    m_optionsToolBar->setObjectName("toolbar.options");
    m_optionsToolBar->setMovable(false);
    m_optionsToolBar->setFixedHeight(56);
    m_optionsToolBar->setStyleSheet("QToolBar { background: #1e1e20; border-bottom: 1px solid #141416; spacing: 0px; padding: 0px 8px; }");

    m_optionsStack = new QStackedWidget(m_optionsToolBar);
    m_optionsStack->setObjectName("optionsStack");
    m_optionsStack->setFixedHeight(38);

    const QString titleStyle = "color: #ffffff; font-weight: 600; font-size: 12px; margin-right: 6px;";
    const QString labelStyle = "color: #a0a0a5; font-size: 11px; margin-left: 4px; margin-right: 2px;";
    const QString spinStyle = "QSpinBox, QDoubleSpinBox { background: #28282b; color: #e0e0e0; border: 1px solid #3c3c40; border-radius: 4px; padding: 2px 4px; font-size: 11px; } "
                              "QSpinBox::up-button, QSpinBox::down-button { width: 0px; }";
    const QString btnStyle = "QPushButton { background: #2c2c2f; color: #d0d0d5; border: 1px solid #3c3c40; border-radius: 4px; padding: 3px 8px; font-size: 11px; } "
                             "QPushButton:hover { background: #38383c; color: #ffffff; } "
                             "QPushButton:pressed { background: #202022; }";
    const QString pillActive = "background: #007aff; color: #ffffff; border: 1px solid #0062cc; border-radius: 4px; padding: 2px 8px; font-size: 11px; font-weight: 500;";
    const QString pillInactive = "background: #28282b; color: #a0a0a5; border: 1px solid #38383c; border-radius: 4px; padding: 2px 8px; font-size: 11px;";

    // Page 0: Move (Tool::Move = 0)
    auto *pageMove = new QWidget(m_optionsStack);
    auto *layoutMove = new QHBoxLayout(pageMove);
    layoutMove->setContentsMargins(0, 0, 0, 0);
    layoutMove->setSpacing(8);

    auto *lblMoveTitle = new QLabel(tr("Move"), pageMove);
    lblMoveTitle->setStyleSheet(titleStyle);
    layoutMove->addWidget(lblMoveTitle);

    auto *chkAutoSelect = new QCheckBox(tr("Auto Select"), pageMove);
    chkAutoSelect->setObjectName("transformAutoSelect");
    chkAutoSelect->setChecked(true);
    m_autoSelectCheck = chkAutoSelect;
    layoutMove->addWidget(chkAutoSelect);

    auto *comboAutoSelectType = new QComboBox(pageMove);
    comboAutoSelectType->addItems({tr("Layer"), tr("Group")});
    layoutMove->addWidget(comboAutoSelectType);

    auto *chkShowControls = new QCheckBox(tr("Show Controls"), pageMove);
    chkShowControls->setChecked(true);
    chkShowControls->setObjectName("transformShowControls");
    m_showControlsCheck = chkShowControls;
    connect(chkShowControls, &QCheckBox::toggled, this, [this] { if (m_canvasWidget) m_canvasWidget->update(); });
    layoutMove->addWidget(chkShowControls);

    auto addCoordBox = [&](const QString &label, int defVal) {
        auto *lbl = new QLabel(label, pageMove);
        lbl->setStyleSheet(labelStyle);
        layoutMove->addWidget(lbl);
        auto *spin = new QSpinBox(pageMove);
        spin->setRange(-9999, 9999);
        spin->setValue(defVal);
        spin->setFixedWidth(56);
        spin->setStyleSheet(spinStyle);
        layoutMove->addWidget(spin);
        return spin;
    };
    m_xSpin = addCoordBox("X", 0);
    m_ySpin = addCoordBox("Y", 0);
    m_wSpin = addCoordBox("W", 512);
    m_hSpin = addCoordBox("H", 512);
    m_xSpin->setObjectName("transform.x");
    m_ySpin->setObjectName("transform.y");
    m_wSpin->setObjectName("transform.w");
    m_hSpin->setObjectName("transform.h");
    m_wSpin->setRange(1, 30000);
    m_hSpin->setRange(1, 30000);
    m_xSpin->setRange(-30000, 30000);
    m_ySpin->setRange(-30000, 30000);
    m_xSpin->setFixedWidth(64); m_ySpin->setFixedWidth(64); m_wSpin->setFixedWidth(64); m_hSpin->setFixedWidth(64);

    auto *chkLink = new QCheckBox(tr("Link"), pageMove);
    chkLink->setChecked(true);
    chkLink->setObjectName("transformLink");
    m_linkCheck = chkLink;
    layoutMove->addWidget(chkLink);

    auto *lblAngle = new QLabel(tr("Angle"), pageMove);
    lblAngle->setStyleSheet(labelStyle);
    layoutMove->addWidget(lblAngle);
    auto *spinAngle = new QSpinBox(pageMove);
    spinAngle->setObjectName("transform.angle");
    m_angleSpin = spinAngle;
    spinAngle->setRange(-360, 360);
    spinAngle->setValue(0);
    spinAngle->setSuffix(QString::fromUtf8("°"));
    spinAngle->setFixedWidth(50);
    spinAngle->setStyleSheet(spinStyle);
    layoutMove->addWidget(spinAngle);
    for (QSpinBox *spin : {m_xSpin, m_ySpin, m_wSpin, m_hSpin, m_angleSpin}) {
        const int field = spin == m_xSpin ? 0 : spin == m_ySpin ? 1 : spin == m_wSpin ? 2 : spin == m_hSpin ? 3 : 4;
        spin->setKeyboardTracking(false);
        connect(spin, &QSpinBox::valueChanged, this, [this, field] { applyTransformFields(field); });
    }
    layoutMove->addStretch();
    m_optionsStack->addWidget(pageMove);

    // Page 1: Marquee (Tool::Marquee = 1)
    auto *pageMarquee = new QWidget(m_optionsStack);
    auto *layoutMarquee = new QHBoxLayout(pageMarquee);
    layoutMarquee->setContentsMargins(0, 0, 0, 0);
    layoutMarquee->setSpacing(8);

    auto *lblMarqueeTitle = new QLabel(tr("Marquee"), pageMarquee);
    lblMarqueeTitle->setStyleSheet(titleStyle);
    layoutMarquee->addWidget(lblMarqueeTitle);

    auto *marqueeGroup = new QWidget(pageMarquee);
    marqueeGroup->setObjectName("select.marqueeGroup");
    auto *mqLayout = new QHBoxLayout(marqueeGroup);
    mqLayout->setContentsMargins(0, 0, 0, 0);
    mqLayout->setSpacing(4);
    auto *btnMqRect = new QPushButton(tr("Rectangle"), marqueeGroup);
    btnMqRect->setStyleSheet(m_marqueeMode == MarqueeMode::Rectangle ? pillActive : pillInactive);
    auto *btnMqEllipse = new QPushButton(tr("Ellipse"), marqueeGroup);
    btnMqEllipse->setStyleSheet(m_marqueeMode == MarqueeMode::Ellipse ? pillActive : pillInactive);
    connect(btnMqRect, &QPushButton::clicked, this, [this, btnMqRect, btnMqEllipse, pillActive, pillInactive] {
        btnMqRect->setStyleSheet(pillActive); btnMqEllipse->setStyleSheet(pillInactive);
        setMarqueeMode(MarqueeMode::Rectangle);
    });
    connect(btnMqEllipse, &QPushButton::clicked, this, [this, btnMqRect, btnMqEllipse, pillActive, pillInactive] {
        btnMqRect->setStyleSheet(pillInactive); btnMqEllipse->setStyleSheet(pillActive);
        setMarqueeMode(MarqueeMode::Ellipse);
    });
    mqLayout->addWidget(btnMqRect);
    mqLayout->addWidget(btnMqEllipse);
    layoutMarquee->addWidget(marqueeGroup);

    auto makeCombineButtons = [&](QWidget *parent, QHBoxLayout *layout) {
        auto *btnNew = new QPushButton(tr("New"), parent);
        btnNew->setStyleSheet(pillActive);
        auto *btnAdd = new QPushButton(tr("Add"), parent);
        btnAdd->setStyleSheet(pillInactive);
        auto *btnSub = new QPushButton(tr("Subtract"), parent);
        btnSub->setStyleSheet(pillInactive);
        connect(btnNew, &QPushButton::clicked, this, [this, btnNew, btnAdd, btnSub, pillActive, pillInactive] {
            m_selectionMode = "New";
            btnNew->setStyleSheet(pillActive); btnAdd->setStyleSheet(pillInactive); btnSub->setStyleSheet(pillInactive);
        });
        connect(btnAdd, &QPushButton::clicked, this, [this, btnNew, btnAdd, btnSub, pillActive, pillInactive] {
            m_selectionMode = "Add";
            btnNew->setStyleSheet(pillInactive); btnAdd->setStyleSheet(pillActive); btnSub->setStyleSheet(pillInactive);
        });
        connect(btnSub, &QPushButton::clicked, this, [this, btnNew, btnAdd, btnSub, pillActive, pillInactive] {
            m_selectionMode = "Subtract";
            btnNew->setStyleSheet(pillInactive); btnAdd->setStyleSheet(pillInactive); btnSub->setStyleSheet(pillActive);
        });
        layout->addWidget(btnNew);
        layout->addWidget(btnAdd);
        layout->addWidget(btnSub);
    };
    makeCombineButtons(pageMarquee, layoutMarquee);

    auto *chkMqAA = new QCheckBox(tr("Anti-alias"), pageMarquee);
    chkMqAA->setChecked(true);
    layoutMarquee->addWidget(chkMqAA);

    auto *btnExpand = new QPushButton(tr("Expand"), pageMarquee);
    btnExpand->setStyleSheet(btnStyle);
    layoutMarquee->addWidget(btnExpand);
    auto *spinExpand = new QSpinBox(pageMarquee);
    spinExpand->setRange(1, 250);
    spinExpand->setValue(1);
    spinExpand->setSuffix(tr(" px"));
    spinExpand->setFixedWidth(54);
    spinExpand->setStyleSheet(spinStyle);
    layoutMarquee->addWidget(spinExpand);
    connect(btnExpand, &QPushButton::clicked, this, [this, spinExpand] {
        if (sendCommand({{"action", "expandSelection"}, {"parameters", QJsonObject{{"amount", spinExpand->value()}}}})) refreshImage();
    });




    auto *btnContract = new QPushButton(tr("Contract"), pageMarquee);
    btnContract->setStyleSheet(btnStyle);
    layoutMarquee->addWidget(btnContract);
    auto *spinContract = new QSpinBox(pageMarquee);
    spinContract->setRange(1, 250);
    spinContract->setValue(1);
    spinContract->setSuffix(tr(" px"));
    spinContract->setFixedWidth(54);
    spinContract->setStyleSheet(spinStyle);
    layoutMarquee->addWidget(spinContract);
    connect(btnContract, &QPushButton::clicked, this, [this, spinContract] {
        if (sendCommand({{"action", "contractSelection"}, {"parameters", QJsonObject{{"amount", spinContract->value()}}}})) refreshImage();
    });

    auto *btnMqDeselect = new QPushButton(tr("Deselect"), pageMarquee);
    btnMqDeselect->setStyleSheet(btnStyle);
    connect(btnMqDeselect, &QPushButton::clicked, this, [this] {
        cmd(m_sessionHandle, R"({"version":1,"action":"deselect"})");
        refreshImage();
    });
    layoutMarquee->addWidget(btnMqDeselect);


    layoutMarquee->addStretch();
    m_optionsStack->addWidget(pageMarquee);

    // Page 2: Lasso (Tool::Lasso = 2)
    auto *pageLasso = new QWidget(m_optionsStack);
    auto *layoutLasso = new QHBoxLayout(pageLasso);
    layoutLasso->setContentsMargins(0, 0, 0, 0);
    layoutLasso->setSpacing(8);

    auto *lblLassoTitle = new QLabel(tr("Lasso"), pageLasso);
    lblLassoTitle->setStyleSheet(titleStyle);
    layoutLasso->addWidget(lblLassoTitle);

    auto *lassoGroup = new QWidget(pageLasso);
    lassoGroup->setObjectName("select.lassoGroup");
    auto *lsLayout = new QHBoxLayout(lassoGroup);
    lsLayout->setContentsMargins(0, 0, 0, 0);
    lsLayout->setSpacing(4);
    auto *btnFreehand = new QPushButton(tr("Freehand"), lassoGroup);
    btnFreehand->setObjectName("select.freehand");
    btnFreehand->setStyleSheet(pillActive);
    auto *btnPolygonal = new QPushButton(tr("Polygonal"), lassoGroup);
    btnPolygonal->setObjectName("select.polygonal");
    btnPolygonal->setStyleSheet(pillInactive);
    connect(btnFreehand, &QPushButton::clicked, this, [this, btnFreehand, btnPolygonal, pillActive, pillInactive] {
        m_polygonalLasso = false; m_polyActive = false; m_lassoPoints.clear();
        btnFreehand->setStyleSheet(pillActive); btnPolygonal->setStyleSheet(pillInactive);
        if (m_canvasWidget) m_canvasWidget->update();
    });
    connect(btnPolygonal, &QPushButton::clicked, this, [this, btnFreehand, btnPolygonal, pillActive, pillInactive] {
        m_polygonalLasso = true; m_polyActive = false; m_lassoPoints.clear();
        btnFreehand->setStyleSheet(pillInactive); btnPolygonal->setStyleSheet(pillActive);
        if (m_canvasWidget) m_canvasWidget->update();
    });
    lsLayout->addWidget(btnFreehand);
    lsLayout->addWidget(btnPolygonal);
    layoutLasso->addWidget(lassoGroup);

    makeCombineButtons(pageLasso, layoutLasso);

    auto *chkLassoAA = new QCheckBox(tr("Anti-alias"), pageLasso);
    chkLassoAA->setChecked(true);
    layoutLasso->addWidget(chkLassoAA);

    auto *btnLsDeselect = new QPushButton(tr("Deselect"), pageLasso);
    btnLsDeselect->setStyleSheet(btnStyle);
    connect(btnLsDeselect, &QPushButton::clicked, this, [this] {
        cmd(m_sessionHandle, R"({"version":1,"action":"deselect"})");
        refreshImage();
    });
    layoutLasso->addWidget(btnLsDeselect);

    layoutLasso->addStretch();
    m_optionsStack->addWidget(pageLasso);

    // Page 3: Magic (Tool::Magic = 3)
    auto *pageMagic = new QWidget(m_optionsStack);
    auto *layoutMagic = new QHBoxLayout(pageMagic);
    layoutMagic->setContentsMargins(0, 0, 0, 0);
    layoutMagic->setSpacing(8);

    auto *lblMagicTitle = new QLabel(tr("Magic"), pageMagic);
    lblMagicTitle->setStyleSheet(titleStyle);
    layoutMagic->addWidget(lblMagicTitle);

    auto *btnWand = new QPushButton(tr("Wand"), pageMagic);
    btnWand->setStyleSheet(m_magicMode == MagicMode::Wand ? pillActive : pillInactive);
    auto *btnObj = new QPushButton(tr("Object"), pageMagic);
    btnObj->setStyleSheet(m_magicMode == MagicMode::Object ? pillActive : pillInactive);
    connect(btnWand, &QPushButton::clicked, this, [this, btnWand, btnObj, pillActive, pillInactive] {
        btnWand->setStyleSheet(pillActive); btnObj->setStyleSheet(pillInactive);
        setMagicMode(MagicMode::Wand);
    });
    connect(btnObj, &QPushButton::clicked, this, [this, btnWand, btnObj, pillActive, pillInactive] {
        btnWand->setStyleSheet(pillInactive); btnObj->setStyleSheet(pillActive);
        setMagicMode(MagicMode::Object);
    });
    layoutMagic->addWidget(btnWand);
    layoutMagic->addWidget(btnObj);

    makeCombineButtons(pageMagic, layoutMagic);

    auto *lblTol = new QLabel(tr("Tolerance"), pageMagic);
    lblTol->setStyleSheet(labelStyle);
    layoutMagic->addWidget(lblTol);
    auto *spinTol = new QSpinBox(pageMagic);
    spinTol->setRange(0, 255);
    spinTol->setValue(m_magicTolerance);
    spinTol->setFixedWidth(50);
    spinTol->setStyleSheet(spinStyle);
    connect(spinTol, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) {
        m_magicTolerance = val;
    });
    layoutMagic->addWidget(spinTol);

    auto *chkWandContig = new QCheckBox(tr("Contiguous"), pageMagic);
    chkWandContig->setChecked(m_magicContiguous);
    connect(chkWandContig, &QCheckBox::toggled, this, [this](bool val) {
        m_magicContiguous = val;
    });
    layoutMagic->addWidget(chkWandContig);

    auto *chkWandSample = new QCheckBox(tr("Sample All Layers"), pageMagic);
    chkWandSample->setChecked(m_magicSampleAll);
    connect(chkWandSample, &QCheckBox::toggled, this, [this](bool val) {
        m_magicSampleAll = val;
    });
    layoutMagic->addWidget(chkWandSample);

    auto *btnMgDeselect = new QPushButton(tr("Deselect"), pageMagic);
    btnMgDeselect->setStyleSheet(btnStyle);
    connect(btnMgDeselect, &QPushButton::clicked, this, [this] {
        cmd(m_sessionHandle, R"({"version":1,"action":"deselect"})");
        refreshImage();
    });
    layoutMagic->addWidget(btnMgDeselect);

    layoutMagic->addStretch();
    m_optionsStack->addWidget(pageMagic);

    // Page 4: Crop (Tool::Crop = 4)
    auto *pageCrop = new QWidget(m_optionsStack);
    auto *layoutCrop = new QHBoxLayout(pageCrop);
    layoutCrop->setContentsMargins(0, 0, 0, 0);
    layoutCrop->setSpacing(8);

    auto *lblCropTitle = new QLabel(tr("Crop"), pageCrop);
    lblCropTitle->setStyleSheet(titleStyle);
    layoutCrop->addWidget(lblCropTitle);

    auto *lblRatio = new QLabel(tr("Ratio"), pageCrop);
    lblRatio->setStyleSheet(labelStyle);
    layoutCrop->addWidget(lblRatio);
    auto *comboRatio = new QComboBox(pageCrop);
    comboRatio->addItems({tr("Free"), tr("Original"), tr("1:1"), tr("4:3"), tr("16:9")});
    connect(comboRatio, &QComboBox::currentTextChanged, this, [this](const QString &ratio) {
        m_cropRatio = ratio;
    });
    layoutCrop->addWidget(comboRatio);

    auto *btnApplyCrop = new QPushButton(tr("Apply Crop"), pageCrop);
    btnApplyCrop->setStyleSheet(btnStyle);
    connect(btnApplyCrop, &QPushButton::clicked, this, &SessionWindow::applyCrop);
    auto *btnCancelCrop = new QPushButton(tr("Cancel"), pageCrop);
    btnCancelCrop->setStyleSheet(btnStyle);
    connect(btnCancelCrop, &QPushButton::clicked, this, &SessionWindow::cancelCrop);
    layoutCrop->addWidget(btnApplyCrop);
    layoutCrop->addWidget(btnCancelCrop);

    layoutCrop->addStretch();
    m_optionsStack->addWidget(pageCrop);

    // Page 5: Brush (Tool::Brush = 5)
    auto *pageBrush = new QWidget(m_optionsStack);
    auto *layoutBrush = new QHBoxLayout(pageBrush);
    layoutBrush->setContentsMargins(0, 0, 0, 0);
    layoutBrush->setSpacing(8);

    auto *lblBrushTitle = new QLabel(tr("Brush"), pageBrush);
    lblBrushTitle->setObjectName("options.brushTitle");
    lblBrushTitle->setStyleSheet(titleStyle);
    layoutBrush->addWidget(lblBrushTitle);

    auto *btnPaintMode = new QPushButton(tr("Paint"), pageBrush);
    btnPaintMode->setStyleSheet(pillActive);
    auto *btnEraseMode = new QPushButton(tr("Erase"), pageBrush);
    btnEraseMode->setStyleSheet(pillInactive);
    connect(btnPaintMode, &QPushButton::clicked, this, [this, btnPaintMode, btnEraseMode, pillActive, pillInactive] {
        btnPaintMode->setStyleSheet(pillActive); btnEraseMode->setStyleSheet(pillInactive);
        setBrushToolMode(BrushToolMode::Paint);
    });
    connect(btnEraseMode, &QPushButton::clicked, this, [this, btnPaintMode, btnEraseMode, pillActive, pillInactive] {
        btnPaintMode->setStyleSheet(pillInactive); btnEraseMode->setStyleSheet(pillActive);
        setBrushToolMode(BrushToolMode::Erase);
    });
    layoutBrush->addWidget(btnPaintMode);
    layoutBrush->addWidget(btnEraseMode);

    auto *lblSize = new QLabel(tr("Size"), pageBrush);
    lblSize->setStyleSheet(labelStyle);
    layoutBrush->addWidget(lblSize);

    m_brushDiameterSlider = new QSlider(Qt::Horizontal, pageBrush);
    m_brushDiameterSlider->setObjectName("brush.diameter");
    m_brushDiameterSlider->setRange(1, 256);
    m_brushDiameterSlider->setValue(m_brushDiameter);
    m_brushDiameterSlider->setFixedWidth(100);
    layoutBrush->addWidget(m_brushDiameterSlider);

    auto *spinSize = new QSpinBox(pageBrush);
    spinSize->setRange(1, 2000);
    spinSize->setValue(m_brushDiameter);
    spinSize->setSuffix(tr(" px"));
    spinSize->setFixedWidth(64);
    spinSize->setStyleSheet(spinStyle);
    layoutBrush->addWidget(spinSize);

    connect(m_brushDiameterSlider, &QSlider::valueChanged, this, [this, spinSize](int val) {
        if (spinSize->value() != val) spinSize->setValue(val);
        setBrushDiameter(val);
    });
    connect(spinSize, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) {
        if (m_brushDiameterSlider->value() != std::min(val, 256)) m_brushDiameterSlider->setValue(std::min(val, 256));
        setBrushDiameter(val);
    });

    auto *lblHard = new QLabel(tr("Hardness"), pageBrush);
    lblHard->setStyleSheet(labelStyle);
    layoutBrush->addWidget(lblHard);

    m_brushHardnessSlider = new QSlider(Qt::Horizontal, pageBrush);
    m_brushHardnessSlider->setObjectName("brush.hardness");
    m_brushHardnessSlider->setRange(0, 100);
    m_brushHardnessSlider->setValue(m_brushHardness);
    m_brushHardnessSlider->setFixedWidth(80);
    layoutBrush->addWidget(m_brushHardnessSlider);

    auto *spinHard = new QSpinBox(pageBrush);
    spinHard->setRange(0, 100);
    spinHard->setValue(m_brushHardness);
    spinHard->setSuffix(tr(" %"));
    spinHard->setFixedWidth(54);
    spinHard->setStyleSheet(spinStyle);
    layoutBrush->addWidget(spinHard);

    connect(m_brushHardnessSlider, &QSlider::valueChanged, this, [this, spinHard](int val) {
        if (spinHard->value() != val) spinHard->setValue(val);
        setBrushHardness(val);
    });
    connect(spinHard, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) {
        if (m_brushHardnessSlider->value() != val) m_brushHardnessSlider->setValue(val);
        setBrushHardness(val);
    });

    auto *lblOpac = new QLabel(tr("Opacity"), pageBrush);
    lblOpac->setStyleSheet(labelStyle);
    layoutBrush->addWidget(lblOpac);

    m_brushOpacitySlider = new QSlider(Qt::Horizontal, pageBrush);
    m_brushOpacitySlider->setObjectName("brush.opacity");
    m_brushOpacitySlider->setRange(0, 100);
    m_brushOpacitySlider->setValue(m_brushOpacity);
    m_brushOpacitySlider->setFixedWidth(80);
    layoutBrush->addWidget(m_brushOpacitySlider);

    auto *spinOpac = new QSpinBox(pageBrush);
    spinOpac->setRange(0, 100);
    spinOpac->setValue(m_brushOpacity);
    spinOpac->setSuffix(tr(" %"));
    spinOpac->setFixedWidth(54);
    spinOpac->setStyleSheet(spinStyle);
    layoutBrush->addWidget(spinOpac);

    connect(m_brushOpacitySlider, &QSlider::valueChanged, this, [this, spinOpac](int val) {
        if (spinOpac->value() != val) spinOpac->setValue(val);
        setBrushOpacity(val);
    });
    connect(spinOpac, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) {
        if (m_brushOpacitySlider->value() != val) m_brushOpacitySlider->setValue(val);
        setBrushOpacity(val);
    });

    auto *lblColor = new QLabel(tr("Color"), pageBrush);
    lblColor->setStyleSheet(labelStyle);
    layoutBrush->addWidget(lblColor);

    auto *fallbackColorButton = new QPushButton(pageBrush);
    fallbackColorButton->setObjectName("brush.color");
    fallbackColorButton->setFixedSize(36, 20);
    fallbackColorButton->setToolTip(tr("Foreground brush color"));
    fallbackColorButton->setStyleSheet(QString("background-color: %1; border: 1px solid #101012; border-radius: 3px;").arg(m_brushColor.name()));
    connect(fallbackColorButton, &QPushButton::clicked, this, &SessionWindow::pickBrushColor);
    layoutBrush->addWidget(fallbackColorButton);

    layoutBrush->addStretch();
    m_optionsStack->addWidget(pageBrush);

    // Page 6: Spot Healing (Tool::SpotHealing = 6)
    auto *pageSpot = new QWidget(m_optionsStack);
    auto *layoutSpot = new QHBoxLayout(pageSpot);
    layoutSpot->setContentsMargins(0, 0, 0, 0);
    layoutSpot->setSpacing(8);

    auto *lblSpotTitle = new QLabel(tr("Spot Healing"), pageSpot);
    lblSpotTitle->setStyleSheet(titleStyle);
    layoutSpot->addWidget(lblSpotTitle);

    auto *lblSpotMode = new QLabel(tr("Mode:"), pageSpot);
    lblSpotMode->setStyleSheet(labelStyle);
    layoutSpot->addWidget(lblSpotMode);
    auto *comboSpotMode = new QComboBox(pageSpot);
    comboSpotMode->addItems({tr("Content-Aware"), tr("Create Texture"), tr("Proximity Match")});
    connect(comboSpotMode, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this](int idx) {
        if (idx == 0) setSpotHealingMode(SpotHealingMode::ContentAware);
        else if (idx == 1) setSpotHealingMode(SpotHealingMode::CreateTexture);
        else setSpotHealingMode(SpotHealingMode::ProximityMatch);
    });
    layoutSpot->addWidget(comboSpotMode);

    auto *lblSpotSize = new QLabel(tr("Size"), pageSpot);
    lblSpotSize->setStyleSheet(labelStyle);
    layoutSpot->addWidget(lblSpotSize);
    auto *sliderSpotSize = new QSlider(Qt::Horizontal, pageSpot);
    sliderSpotSize->setRange(1, 256);
    sliderSpotSize->setValue(m_brushDiameter);
    sliderSpotSize->setFixedWidth(90);
    layoutSpot->addWidget(sliderSpotSize);
    auto *spinSpotSize = new QSpinBox(pageSpot);
    spinSpotSize->setRange(1, 2000);
    spinSpotSize->setValue(m_brushDiameter);
    spinSpotSize->setSuffix(tr(" px"));
    spinSpotSize->setFixedWidth(60);
    spinSpotSize->setStyleSheet(spinStyle);
    layoutSpot->addWidget(spinSpotSize);
    connect(sliderSpotSize, &QSlider::valueChanged, this, [this, spinSpotSize](int val) {
        spinSpotSize->setValue(val); setBrushDiameter(val);
    });
    connect(spinSpotSize, QOverload<int>::of(&QSpinBox::valueChanged), this, [this, sliderSpotSize](int val) {
        sliderSpotSize->setValue(std::min(val, 256)); setBrushDiameter(val);
    });

    layoutSpot->addStretch();
    m_optionsStack->addWidget(pageSpot);

    // Page 7: Clone Stamp (Tool::CloneStamp = 7)
    auto *pageClone = new QWidget(m_optionsStack);
    auto *layoutClone = new QHBoxLayout(pageClone);
    layoutClone->setContentsMargins(0, 0, 0, 0);
    layoutClone->setSpacing(8);

    auto *lblCloneTitle = new QLabel(tr("Clone Stamp"), pageClone);
    lblCloneTitle->setObjectName("options.cloneTitle");
    lblCloneTitle->setStyleSheet(titleStyle);
    layoutClone->addWidget(lblCloneTitle);

    auto *chkAligned = new QCheckBox(tr("Aligned"), pageClone);
    chkAligned->setChecked(m_cloneAligned);
    connect(chkAligned, &QCheckBox::toggled, this, [this](bool val) { m_cloneAligned = val; });
    layoutClone->addWidget(chkAligned);

    auto *lblSample = new QLabel(tr("Sample:"), pageClone);
    lblSample->setStyleSheet(labelStyle);
    layoutClone->addWidget(lblSample);
    auto *comboSample = new QComboBox(pageClone);
    comboSample->addItems({tr("Current Layer"), tr("All Layers")});
    connect(comboSample, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this](int idx) {
        m_cloneSampleAll = (idx == 1);
    });
    layoutClone->addWidget(comboSample);

    layoutClone->addStretch();
    m_optionsStack->addWidget(pageClone);

    // Page 8: Smear (Tool::Smear = 8)
    auto *pageSmear = new QWidget(m_optionsStack);
    auto *layoutSmear = new QHBoxLayout(pageSmear);
    layoutSmear->setContentsMargins(0, 0, 0, 0);
    layoutSmear->setSpacing(8);

    auto *lblSmearTitle = new QLabel(tr("Smear"), pageSmear);
    lblSmearTitle->setStyleSheet(titleStyle);
    layoutSmear->addWidget(lblSmearTitle);

    auto *lblSmearMode = new QLabel(tr("Mode:"), pageSmear);
    lblSmearMode->setStyleSheet(labelStyle);
    layoutSmear->addWidget(lblSmearMode);
    auto *comboSmearMode = new QComboBox(pageSmear);
    comboSmearMode->addItems({tr("Liquify"), tr("Blur"), tr("Smudge")});
    connect(comboSmearMode, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this](int idx) {
        if (idx == 0) setSmearMode(SmearMode::Liquify);
        else if (idx == 1) setSmearMode(SmearMode::Blur);
        else setSmearMode(SmearMode::Smudge);
    });
    layoutSmear->addWidget(comboSmearMode);

    auto *lblSmearSize = new QLabel(tr("Size"), pageSmear);
    lblSmearSize->setStyleSheet(labelStyle);
    layoutSmear->addWidget(lblSmearSize);
    auto *sliderSmearSize = new QSlider(Qt::Horizontal, pageSmear);
    sliderSmearSize->setRange(1, 256);
    sliderSmearSize->setValue(m_brushDiameter);
    sliderSmearSize->setFixedWidth(90);
    layoutSmear->addWidget(sliderSmearSize);
    connect(sliderSmearSize, &QSlider::valueChanged, this, [this](int val) { setBrushDiameter(val); });

    layoutSmear->addStretch();
    m_optionsStack->addWidget(pageSmear);

    // Page 9: Gradient (Tool::Gradient = 9)
    auto *pageGradient = new QWidget(m_optionsStack);
    auto *layoutGradient = new QHBoxLayout(pageGradient);
    layoutGradient->setContentsMargins(0, 0, 0, 0);
    layoutGradient->setSpacing(8);

    auto *lblGradTitle = new QLabel(tr("Gradient"), pageGradient);
    lblGradTitle->setStyleSheet(titleStyle);
    layoutGradient->addWidget(lblGradTitle);

    auto *comboGradType = new QComboBox(pageGradient);
    comboGradType->addItems({tr("Linear"), tr("Radial"), tr("Angle"), tr("Reflected"), tr("Diamond")});
    connect(comboGradType, &QComboBox::currentTextChanged, this, [this](const QString &t) { m_gradientType = t; });
    layoutGradient->addWidget(comboGradType);

    auto *lblGradBlend = new QLabel(tr("Blend:"), pageGradient);
    lblGradBlend->setStyleSheet(labelStyle);
    layoutGradient->addWidget(lblGradBlend);
    auto *comboGradBlend = new QComboBox(pageGradient);
    comboGradBlend->addItems(blendModes());
    layoutGradient->addWidget(comboGradBlend);

    auto *lblGradOpac = new QLabel(tr("Opacity:"), pageGradient);
    lblGradOpac->setStyleSheet(labelStyle);
    layoutGradient->addWidget(lblGradOpac);
    auto *spinGradOpac = new QSpinBox(pageGradient);
    spinGradOpac->setRange(0, 100);
    spinGradOpac->setValue(m_gradientOpacity);
    spinGradOpac->setSuffix(tr(" %"));
    spinGradOpac->setFixedWidth(54);
    spinGradOpac->setStyleSheet(spinStyle);
    connect(spinGradOpac, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) { m_gradientOpacity = val; });
    layoutGradient->addWidget(spinGradOpac);

    auto *chkGradReverse = new QCheckBox(tr("Reverse"), pageGradient);
    chkGradReverse->setChecked(m_gradientReverse);
    connect(chkGradReverse, &QCheckBox::toggled, this, [this](bool val) { m_gradientReverse = val; });
    layoutGradient->addWidget(chkGradReverse);

    layoutGradient->addStretch();
    m_optionsStack->addWidget(pageGradient);

    // Page 10: Shape (Tool::Shape = 10)
    auto *pageShape = new QWidget(m_optionsStack);
    auto *layoutShape = new QHBoxLayout(pageShape);
    layoutShape->setContentsMargins(0, 0, 0, 0);
    layoutShape->setSpacing(8);

    auto *lblShapeTitle = new QLabel(tr("Shape"), pageShape);
    lblShapeTitle->setStyleSheet(titleStyle);
    layoutShape->addWidget(lblShapeTitle);

    auto *comboShapeMode = new QComboBox(pageShape);
    comboShapeMode->addItems({tr("Rectangle"), tr("Ellipse"), tr("Line")});
    connect(comboShapeMode, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this](int idx) {
        if (idx == 0) setShapeMode(ShapeMode::Rectangle);
        else if (idx == 1) setShapeMode(ShapeMode::Ellipse);
        else setShapeMode(ShapeMode::Line);
    });
    layoutShape->addWidget(comboShapeMode);

    auto *lblStrokeW = new QLabel(tr("Stroke:"), pageShape);
    lblStrokeW->setStyleSheet(labelStyle);
    layoutShape->addWidget(lblStrokeW);
    auto *spinStrokeW = new QSpinBox(pageShape);
    spinStrokeW->setRange(0, 100);
    spinStrokeW->setValue(m_shapeStrokeWidth);
    spinStrokeW->setSuffix(tr(" px"));
    spinStrokeW->setFixedWidth(64);
    spinStrokeW->setStyleSheet(spinStyle);
    connect(spinStrokeW, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) { m_shapeStrokeWidth = val; });
    layoutShape->addWidget(spinStrokeW);

    auto *lblRadius = new QLabel(tr("Radius:"), pageShape);
    lblRadius->setStyleSheet(labelStyle);
    layoutShape->addWidget(lblRadius);
    auto *spinRadius = new QSpinBox(pageShape);
    spinRadius->setRange(0, 100);
    spinRadius->setValue(m_shapeRadius);
    spinRadius->setSuffix(tr(" px"));
    spinRadius->setFixedWidth(54);
    spinRadius->setStyleSheet(spinStyle);
    connect(spinRadius, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) { m_shapeRadius = val; });
    layoutShape->addWidget(spinRadius);

    layoutShape->addStretch();
    m_optionsStack->addWidget(pageShape);

    // Page 11: Type (Tool::Type = 11)
    auto *pageType = new QWidget(m_optionsStack);
    auto *layoutType = new QHBoxLayout(pageType);
    layoutType->setContentsMargins(0, 0, 0, 0);
    layoutType->setSpacing(8);

    auto *lblTypeTitle = new QLabel(tr("Type"), pageType);
    lblTypeTitle->setStyleSheet(titleStyle);
    layoutType->addWidget(lblTypeTitle);

    auto *comboFont = new QComboBox(pageType);
    comboFont->addItems({tr("System Default"), tr("Sans Serif"), tr("Serif"), tr("Monospace")});
    layoutType->addWidget(comboFont);

    auto *lblTypeSize = new QLabel(tr("Size:"), pageType);
    lblTypeSize->setStyleSheet(labelStyle);
    layoutType->addWidget(lblTypeSize);
    auto *spinTypeSize = new QSpinBox(pageType);
    spinTypeSize->setRange(6, 288);
    spinTypeSize->setValue(m_typeFontSize);
    spinTypeSize->setSuffix(tr(" pt"));
    spinTypeSize->setFixedWidth(58);
    spinTypeSize->setStyleSheet(spinStyle);
    connect(spinTypeSize, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int val) { m_typeFontSize = val; });
    layoutType->addWidget(spinTypeSize);

    auto *btnAlignLeft = new QPushButton(tr("Left"), pageType);
    btnAlignLeft->setStyleSheet(pillActive);
    auto *btnAlignCenter = new QPushButton(tr("Center"), pageType);
    btnAlignCenter->setStyleSheet(pillInactive);
    auto *btnAlignRight = new QPushButton(tr("Right"), pageType);
    btnAlignRight->setStyleSheet(pillInactive);
    connect(btnAlignLeft, &QPushButton::clicked, this, [this, btnAlignLeft, btnAlignCenter, btnAlignRight, pillActive, pillInactive] {
        m_typeAlignment = 0;
        btnAlignLeft->setStyleSheet(pillActive); btnAlignCenter->setStyleSheet(pillInactive); btnAlignRight->setStyleSheet(pillInactive);
    });
    connect(btnAlignCenter, &QPushButton::clicked, this, [this, btnAlignLeft, btnAlignCenter, btnAlignRight, pillActive, pillInactive] {
        m_typeAlignment = 1;
        btnAlignLeft->setStyleSheet(pillInactive); btnAlignCenter->setStyleSheet(pillActive); btnAlignRight->setStyleSheet(pillInactive);
    });
    connect(btnAlignRight, &QPushButton::clicked, this, [this, btnAlignLeft, btnAlignCenter, btnAlignRight, pillActive, pillInactive] {
        m_typeAlignment = 2;
        btnAlignLeft->setStyleSheet(pillInactive); btnAlignCenter->setStyleSheet(pillInactive); btnAlignRight->setStyleSheet(pillActive);
    });
    layoutType->addWidget(btnAlignLeft);
    layoutType->addWidget(btnAlignCenter);
    layoutType->addWidget(btnAlignRight);

    layoutType->addStretch();
    m_optionsStack->addWidget(pageType);

    // Page 12: Eyedropper (Tool::Eyedropper = 12)
    auto *pageEye = new QWidget(m_optionsStack);
    auto *layoutEye = new QHBoxLayout(pageEye);
    layoutEye->setContentsMargins(0, 0, 0, 0);
    layoutEye->setSpacing(8);

    auto *lblEyeTitle = new QLabel(tr("Eyedropper"), pageEye);
    lblEyeTitle->setStyleSheet(titleStyle);
    layoutEye->addWidget(lblEyeTitle);

    auto *lblSampleSz = new QLabel(tr("Sample Size:"), pageEye);
    lblSampleSz->setStyleSheet(labelStyle);
    layoutEye->addWidget(lblSampleSz);
    auto *comboEyeSz = new QComboBox(pageEye);
    comboEyeSz->addItems({tr("Point Sample"), tr("3 by 3 Average"), tr("5 by 5 Average")});
    connect(comboEyeSz, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this](int idx) {
        m_eyedropperSampleSize = idx;
    });
    layoutEye->addWidget(comboEyeSz);

    auto *lblEyeSample = new QLabel(tr("Sample:"), pageEye);
    lblEyeSample->setStyleSheet(labelStyle);
    layoutEye->addWidget(lblEyeSample);
    auto *comboEyeSample = new QComboBox(pageEye);
    comboEyeSample->addItems({tr("All Layers"), tr("Current Layer")});
    connect(comboEyeSample, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this](int idx) {
        m_eyedropperSampleAll = (idx == 0);
    });
    layoutEye->addWidget(comboEyeSample);

    layoutEye->addStretch();
    m_optionsStack->addWidget(pageEye);

    // Page 13: Hand (Tool::Hand = 13)
    auto *pageHand = new QWidget(m_optionsStack);
    auto *layoutHand = new QHBoxLayout(pageHand);
    layoutHand->setContentsMargins(0, 0, 0, 0);
    layoutHand->setSpacing(8);

    auto *lblHandTitle = new QLabel(tr("Hand"), pageHand);
    lblHandTitle->setStyleSheet(titleStyle);
    layoutHand->addWidget(lblHandTitle);

    auto *btnFit = new QPushButton(tr("Fit on Screen"), pageHand);
    btnFit->setStyleSheet(btnStyle);
    connect(btnFit, &QPushButton::clicked, this, &SessionWindow::fitCanvas);
    layoutHand->addWidget(btnFit);

    auto *btnActual = new QPushButton(tr("100%"), pageHand);
    btnActual->setStyleSheet(btnStyle);
    connect(btnActual, &QPushButton::clicked, this, &SessionWindow::actualPixels);
    layoutHand->addWidget(btnActual);

    auto *lblHandHint = new QLabel(tr("Drag canvas to pan · Hold Space to pan from any tool"), pageHand);
    lblHandHint->setStyleSheet(labelStyle);
    layoutHand->addWidget(lblHandHint);

    layoutHand->addStretch();
    m_optionsStack->addWidget(pageHand);

    // Page 14: Zoom (Tool::Zoom = 14)
    auto *pageZoom = new QWidget(m_optionsStack);
    auto *layoutZoom = new QHBoxLayout(pageZoom);
    layoutZoom->setContentsMargins(0, 0, 0, 0);
    layoutZoom->setSpacing(8);

    auto *lblZoomTitle = new QLabel(tr("Zoom"), pageZoom);
    lblZoomTitle->setStyleSheet(titleStyle);
    layoutZoom->addWidget(lblZoomTitle);

    auto *btnZoomInTool = new QPushButton(tr("(+) Zoom In"), pageZoom);
    btnZoomInTool->setStyleSheet(btnStyle);
    connect(btnZoomInTool, &QPushButton::clicked, this, [this] { zoomBy(1.25); });
    layoutZoom->addWidget(btnZoomInTool);

    auto *btnZoomOutTool = new QPushButton(tr("(-) Zoom Out"), pageZoom);
    btnZoomOutTool->setStyleSheet(btnStyle);
    connect(btnZoomOutTool, &QPushButton::clicked, this, [this] { zoomBy(0.8); });
    layoutZoom->addWidget(btnZoomOutTool);

    auto *btnZoomFit = new QPushButton(tr("Fit on Screen"), pageZoom);
    btnZoomFit->setStyleSheet(btnStyle);
    connect(btnZoomFit, &QPushButton::clicked, this, &SessionWindow::fitCanvas);
    layoutZoom->addWidget(btnZoomFit);

    auto *btnZoom100 = new QPushButton(tr("100%"), pageZoom);
    btnZoom100->setStyleSheet(btnStyle);
    connect(btnZoom100, &QPushButton::clicked, this, &SessionWindow::actualPixels);
    layoutZoom->addWidget(btnZoom100);

    layoutZoom->addStretch();
    m_optionsStack->addWidget(pageZoom);

    // Page 15: Idle (Tool::Idle = 15)
    auto *pageIdle = new QWidget(m_optionsStack);
    auto *layoutIdle = new QHBoxLayout(pageIdle);
    layoutIdle->setContentsMargins(0, 0, 0, 0);
    layoutIdle->setSpacing(8);
    auto *lblIdle = new QLabel(tr("Ready · Select a tool from the tool rail"), pageIdle);
    lblIdle->setStyleSheet(labelStyle);
    layoutIdle->addWidget(lblIdle);
    layoutIdle->addStretch();
    m_optionsStack->addWidget(pageIdle);

    m_optionsToolBar->addWidget(m_optionsStack)->setObjectName("options.nativeAction");

    m_swiftUIOptionsContainer = new QWidget(m_optionsToolBar);
    m_swiftUIOptionsContainer->setObjectName("swiftUIOptionsContainer");
    m_swiftUIOptionsContainer->setFixedHeight(52);
    m_swiftUIOptionsContainer->setMinimumWidth(0);
    m_swiftUIOptionsContainer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    auto *layoutSwiftUI = new QHBoxLayout(m_swiftUIOptionsContainer);
    layoutSwiftUI->setContentsMargins(0, 0, 0, 0);
    layoutSwiftUI->setSpacing(0);
    m_toolOptionsScroll = new QScrollArea(m_swiftUIOptionsContainer);
    m_toolOptionsScroll->setObjectName("options.scroll");
    m_toolOptionsScroll->setFrameShape(QFrame::NoFrame);
    m_toolOptionsScroll->setWidgetResizable(true);
    m_toolOptionsScroll->setMinimumWidth(0);
    m_toolOptionsScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_toolOptionsScroll->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    layoutSwiftUI->addWidget(m_toolOptionsScroll);
    m_optionsToolBar->addWidget(m_swiftUIOptionsContainer)->setObjectName("options.sharedAction");
    m_swiftUIOptionsContainer->hide();
}

void SessionWindow::updateOptionsBar() {
    if (!m_optionsStack) return;
    const int idx = static_cast<int>(m_tool);
    if (idx >= 0 && idx < m_optionsStack->count()) {
        m_optionsStack->setCurrentIndex(idx);
    }
    if (m_tool == Tool::Brush && m_optionsStack->count() > 5) {
        auto *lbl = m_optionsStack->widget(5)->findChild<QLabel *>("options.brushTitle");
        if (lbl) lbl->setText(m_brushToolMode == BrushToolMode::Erase ? tr("Eraser") : tr("Brush"));
    }
    if (m_brushColorButton) {
        m_brushColorButton->setStyleSheet(QString("background-color: %1; border: 1px solid #101012; border-radius: 3px;").arg(m_brushColor.name()));
    }

    if (m_sessionHandle != 0 && m_swiftUIOptionsContainer) {
        QWidget *rendered = swiftUIRenderPanel(m_sessionHandle, QStringLiteral("ToolHeaders"), m_swiftUICurrentToolHeader);
        // QWidgetAction owns toolbar visibility; hiding only its widget lets
        // QToolBar show it again on relayout, hiding the real controls in overflow.
        m_optionsToolBar->findChild<QAction *>("options.nativeAction")->setVisible(!rendered);
        m_optionsToolBar->findChild<QAction *>("options.sharedAction")->setVisible(rendered != nullptr);
        if (rendered == m_swiftUICurrentToolHeader && rendered) return;
        if (rendered) {
            if (m_swiftUICurrentToolHeader) {
                m_toolOptionsScroll->takeWidget();
                m_swiftUICurrentToolHeader->hide();
                m_swiftUICurrentToolHeader->deleteLater();
                m_swiftUICurrentToolHeader = nullptr;
            }
            m_swiftUICurrentToolHeader = rendered;
            m_toolOptionsScroll->setWidget(rendered);
            rendered->show();
            m_swiftUIOptionsContainer->show();
            m_optionsStack->hide();
        } else {
            if (m_swiftUICurrentToolHeader) {
                m_toolOptionsScroll->takeWidget();
                m_swiftUICurrentToolHeader->hide();
                m_swiftUICurrentToolHeader->deleteLater();
                m_swiftUICurrentToolHeader = nullptr;
            }
            m_swiftUIOptionsContainer->hide();
            m_optionsStack->show();
        }
    }
}

void SessionWindow::updateToolRail() {
    if (m_sessionHandle == 0 || !m_toolsBar) return;
    if (!m_swiftUIToolRailContainer) {
        m_swiftUIToolRailContainer = new QWidget(m_toolsBar);
        m_swiftUIToolRailContainer->setObjectName("swiftUIToolRailContainer");
        auto *layout = new QVBoxLayout(m_swiftUIToolRailContainer);
        layout->setContentsMargins(0, 0, 0, 0);
        layout->setSpacing(0);
        m_toolsBar->insertWidget(m_toolsBar->actions().isEmpty() ? nullptr : m_toolsBar->actions().first(), m_swiftUIToolRailContainer);
    }
    QWidget *rendered = swiftUIRenderPanel(m_sessionHandle, QStringLiteral("ToolRail"));
    if (rendered) {
        if (m_swiftUICurrentToolRail) {
            m_swiftUIToolRailContainer->layout()->removeWidget(m_swiftUICurrentToolRail);
            m_swiftUICurrentToolRail->deleteLater();
            m_swiftUICurrentToolRail = nullptr;
        }
        m_swiftUICurrentToolRail = rendered;
        m_swiftUIToolRailContainer->layout()->addWidget(rendered);
        rendered->show();
        m_swiftUIToolRailContainer->show();
        for (auto *act : m_toolActions.values()) act->setVisible(false);
        // Keep the native palette controls; they share the session color state.
    }
}

void SessionWindow::syncToolFromSession() {
    if (m_sessionHandle == 0) return;
    const auto state = sessionState();
    const QString toolStr = state.value("tool").toString();
    if (toolStr.isEmpty()) return;
    Tool t = m_tool;
    if (toolStr == "move") t = Tool::Move;
    else if (toolStr == "marquee") t = Tool::Marquee;
    else if (toolStr == "lasso") t = Tool::Lasso;
    else if (toolStr == "wand") t = Tool::Magic;
    else if (toolStr == "crop") t = Tool::Crop;
    else if (toolStr == "brush") t = Tool::Brush;
    else if (toolStr == "spotHealing") t = Tool::SpotHealing;
    else if (toolStr == "cloneStamp") t = Tool::CloneStamp;
    else if (toolStr == "blur") t = Tool::Smear;
    else if (toolStr == "gradient") t = Tool::Gradient;
    else if (toolStr == "shape") t = Tool::Shape;
    else if (toolStr == "type") t = Tool::Type;
    else if (toolStr == "eyedropper") t = Tool::Eyedropper;
    else if (toolStr == "hand") t = Tool::Hand;
    else if (toolStr == "zoom") t = Tool::Zoom;
    else if (toolStr == "idle") t = Tool::Idle;
    if (t != m_tool) {
        QMetaObject::invokeMethod(this, [this, t] { setTool(t); }, Qt::QueuedConnection);
    }
}

void SessionWindow::updateStatusTelemetry() {
    if (!statusBar()) return;

    if (!m_statusZoomLabel) {
        m_statusZoomLabel = new QLabel(this);
        m_statusZoomLabel->setObjectName("status.zoom");
        m_statusZoomLabel->setStyleSheet("color: #88888c; font-size: 11px; padding: 0 8px;");
        statusBar()->addWidget(m_statusZoomLabel);
    }
    if (!m_statusDimsLabel) {
        m_statusDimsLabel = new QLabel(this);
        m_statusDimsLabel->setObjectName("status.dims");
        m_statusDimsLabel->setStyleSheet("color: #88888c; font-size: 11px; padding: 0 8px; border-left: 1px solid #28282b;");
        statusBar()->addWidget(m_statusDimsLabel);
    }
    if (!m_statusProfileLabel) {
        m_statusProfileLabel = new QLabel(this);
        m_statusProfileLabel->setObjectName("status.profile");
        m_statusProfileLabel->setStyleSheet("color: #88888c; font-size: 11px; padding: 0 8px; border-left: 1px solid #28282b;");
        statusBar()->addWidget(m_statusProfileLabel);
    }
    if (!m_statusHintsLabel) {
        m_statusHintsLabel = new QLabel(this);
        m_statusHintsLabel->setObjectName("status.hints");
        m_statusHintsLabel->setStyleSheet("color: #707074; font-size: 11px; padding: 0 8px;");
        statusBar()->addPermanentWidget(m_statusHintsLabel);
    }

    // Same rect the canvas draws into, so the readout always matches what is on screen.
    double effectiveZoom = 1.0;
    if (!m_image.isNull() && m_canvasWidget) {
        effectiveZoom = canvasTargetRect().width() / m_image.width();
    } else if (m_zoomLevel > 0.0) {
        effectiveZoom = m_zoomLevel;
    }
    m_statusZoomLabel->setText(QString("%1%").arg(effectiveZoom * 100.0, 0, 'f', 1));

    if (!m_image.isNull()) {
        m_statusDimsLabel->setText(QString("%1 × %2 px").arg(m_image.width()).arg(m_image.height()));
    } else {
        m_statusDimsLabel->setText(tr("No canvas"));
    }
    m_statusProfileLabel->setText(tr("sRGB · Transparent"));

    QString hint;
    switch (m_tool) {
    case Tool::Move:
        hint = tr("Click to select · Click outside to deselect · Drag to move · Handles to resize · Space to pan");
        break;
    case Tool::Brush:
        if (m_brushToolMode == BrushToolMode::Erase) {
            hint = tr("Drag on canvas to erase · [ and ] resize eraser · 1–9 opacity · Space to pan");
        } else {
            hint = tr("Drag on canvas to paint · [ and ] resize brush · 1–9 opacity · Space to pan");
        }
        break;
    case Tool::Marquee:
    case Tool::Lasso:
        hint = tr("Drag to select · Drag inside to move · Shift add · Option subtract · Ctrl+D deselect");
        break;
    case Tool::Magic:
        hint = tr("Click to select region · Shift add · Option subtract · Ctrl+D deselect");
        break;
    case Tool::CloneStamp:
        hint = tr("Alt-click to set source · Drag to clone from source point");
        break;
    case Tool::SpotHealing:
        hint = tr("Drag over blemishes to heal using surrounding texture");
        break;
    case Tool::Crop:
        hint = tr("Drag to define crop region · Apply to crop canvas · Esc to cancel");
        break;
    case Tool::Smear:
        hint = tr("Drag to smudge or liquify pixels");
        break;
    case Tool::Gradient:
        hint = tr("Drag to apply gradient");
        break;
    case Tool::Shape:
        hint = tr("Drag to draw shape");
        break;
    case Tool::Type:
        hint = tr("Click on canvas to add text layer");
        break;
    case Tool::Eyedropper:
        hint = tr("Click on canvas to sample color");
        break;
    case Tool::Hand:
        hint = tr("Drag to pan canvas");
        break;
    case Tool::Zoom:
        hint = tr("Click to zoom in · Option-click to zoom out");
        break;
    default:
        hint = tr("Ready");
        break;
    }
    m_statusHintsLabel->setText(hint);

    if (m_sessionHandle != 0) {
        if (!m_swiftUIStatusBarContainer) {
            m_swiftUIStatusBarContainer = new QWidget(this);
            m_swiftUIStatusBarContainer->setObjectName("swiftUIStatusBarContainer");
            auto *layout = new QHBoxLayout(m_swiftUIStatusBarContainer);
            layout->setContentsMargins(0, 0, 0, 0);
            layout->setSpacing(0);
            statusBar()->addWidget(m_swiftUIStatusBarContainer, 1);
        }
        QWidget *rendered = swiftUIRenderPanel(m_sessionHandle, QStringLiteral("StatusBar"));
        if (rendered) {
            if (m_swiftUICurrentStatusBar) {
                m_swiftUIStatusBarContainer->layout()->removeWidget(m_swiftUICurrentStatusBar);
                delete m_swiftUICurrentStatusBar;
                m_swiftUICurrentStatusBar = nullptr;
            }
            m_swiftUICurrentStatusBar = rendered;
            // Qt owns the viewport; the shared session's Mac viewport is not used
            // for canvas fitting, so expose the actual displayed scale here.
            if (auto *zoom = rendered->findChild<QLabel *>("zoomStatus")) zoom->setText(m_statusZoomLabel->text());
            m_swiftUIStatusBarContainer->layout()->addWidget(rendered);
            rendered->show();
            m_swiftUIStatusBarContainer->show();
            m_statusZoomLabel->hide();
            m_statusDimsLabel->hide();
            m_statusProfileLabel->hide();
            m_statusHintsLabel->hide();
        } else {
            if (m_swiftUICurrentStatusBar) {
                m_swiftUIStatusBarContainer->layout()->removeWidget(m_swiftUICurrentStatusBar);
                delete m_swiftUICurrentStatusBar;
                m_swiftUICurrentStatusBar = nullptr;
            }
            m_swiftUIStatusBarContainer->hide();
            m_statusZoomLabel->show();
            m_statusDimsLabel->show();
            m_statusProfileLabel->show();
            m_statusHintsLabel->show();
        }
    }
}

void SessionWindow::fitCanvas() {
    m_zoomLevel = 0.0;
    m_panOffset = QPointF(0, 0);
    updateStatusTelemetry();
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::actualPixels() {
    m_zoomLevel = 1.0;
    m_panOffset = QPointF(0, 0);
    updateStatusTelemetry();
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::zoomBy(double factor) {
    double current = m_zoomLevel;
    if (current <= 0.0 && !m_image.isNull()) {
        const QSize canvasSize = m_canvasWidget ? m_canvasWidget->size() : size();
        const int pad = 24;
        const int maxW = std::max(10, canvasSize.width() - pad * 2);
        const int maxH = std::max(10, canvasSize.height() - pad * 2);
        current = std::min(static_cast<double>(maxW) / m_image.width(),
                           static_cast<double>(maxH) / m_image.height());
    }
    if (current <= 0.0) current = 1.0;
    m_zoomLevel = std::clamp(current * factor, 0.05, 32.0);
    updateStatusTelemetry();
    if (m_canvasWidget) m_canvasWidget->update();
}

void SessionWindow::applyShortcuts() {
    m_shortcuts=sessionState().value("shortcuts").toArray();
    for(auto value:m_shortcuts) {
        const auto d=value.toObject();
        if(d.value("group")=="Menus") {
            const auto name=shortcuts::menus.value(d.value("title").toString());
            if(!name.isEmpty()) if(auto *action=findChild<QAction *>(name))
                action->setShortcut(shortcuts::sequence(d.value("key").toString(),d.value("modifiers").toInt()));
        }
    }
    // Tool keys are handled by the responder, so they never steal text entry.
    for(auto *action:m_toolActions) action->setShortcut(QKeySequence());
}

void SessionWindow::syncSharedControls() {
    const auto state=sessionState();
    const auto selection=state.value("selectionControls").toObject();
    m_selectionMode=selection.value("mode").toString("New");
    m_marqueeMode=selection.value("marquee")=="Ellipse" ? MarqueeMode::Ellipse : MarqueeMode::Rectangle;
    m_polygonalLasso=selection.value("lasso")=="Polygonal";
    m_lassoMode=m_polygonalLasso ? LassoMode::Polygonal : LassoMode::Freehand;
    const auto b=state.value("brush").toObject();
    m_brushDiameter=qRound(b.value("diameter").toDouble(40)); m_brushHardness=qRound(100*b.value("hardness").toDouble(1));
    m_brushOpacity=qRound(100*b.value("opacity").toDouble(1));
    m_brushToolMode=b.value("erasing").toInt() ? BrushToolMode::Erase : BrushToolMode::Paint;
    m_cloneAligned=b.value("aligned").toInt(); m_cloneSampleAll=b.value("sampleAllLayers").toInt();
    m_spotHealingMode=static_cast<SpotHealingMode>(b.value("healingMode").toInt());
    const auto p=state.value("palette").toObject();
    m_brushColor=QColor::fromRgbF(p.value("red").toDouble(),p.value("green").toDouble(),p.value("blue").toDouble());
    m_backgroundColor=QColor::fromRgbF(p.value("backgroundRed").toDouble(),p.value("backgroundGreen").toDouble(),p.value("backgroundBlue").toDouble());
    for(auto *button:findChildren<QPushButton *>("brush.color")) button->setStyleSheet(QString("background:%1;border:1px solid #777;").arg(m_brushColor.name()));
    if(m_bgColorButton) m_bgColorButton->setStyleSheet(QString("background:%1;border:1px solid #777;").arg(m_backgroundColor.name()));
    if(m_tool==Tool::Crop) {
        const auto crop=state.value("cropRect").toArray(); m_hasPendingCrop=crop.size()==4;
        m_pendingCropRect=m_hasPendingCrop ? QRectF(crop[0].toDouble(),crop[1].toDouble(),crop[2].toDouble(),crop[3].toDouble()) : QRectF();
    }
    const auto settings=state.value("toolSettings").toObject();
    m_magicTolerance=settings.value("tolerance").toString().toInt();
    m_magicContiguous=settings.value("contiguous")=="1"; m_magicSampleAll=settings.value("sampleAllLayers")=="1";
    m_magicMode=settings.value("wand")=="Object" ? MagicMode::Object : MagicMode::Wand;
    m_shapeMode=settings.value("shape")=="Ellipse" ? ShapeMode::Ellipse : settings.value("shape")=="Line" ? ShapeMode::Line : ShapeMode::Rectangle;
    m_smearMode=settings.value("blur")=="Blur" ? SmearMode::Blur : settings.value("blur")=="Smudge" ? SmearMode::Smudge : SmearMode::Liquify;
}

void SessionWindow::presentSharedColorPicker() {
    const auto picker=sessionState().value("colorPicker");
    if(!picker.isObject() || m_presentingColor) return;
    m_presentingColor=true;
    const auto p=picker.toObject();
    const auto initial=QColor::fromRgbF(p.value("red").toDouble(),p.value("green").toDouble(),p.value("blue").toDouble());
    const QColor color=m_platform.colors->pick(initial,tr("Color"));
    QJsonObject command{{"action","finishColorPicker"},{"enabled",color.isValid()}};
    if(color.isValid()) command.insert("parameters",QJsonObject{{"red",color.redF()},{"green",color.greenF()},{"blue",color.blueF()}});
    sendCommand(command); m_presentingColor=false;
    syncSharedControls(); updateOptionsBar(); refreshImage();
}

void SessionWindow::showTextEditor() {
    const auto content=sessionState().value("textContent");
    if(!content.isString() || m_presentingText) return;
    m_presentingText=true;
    QDialog dialog(this); dialog.setObjectName("text.dialog"); dialog.setWindowTitle(tr("Edit Text")); dialog.resize(480,260);
    auto *layout=new QVBoxLayout(&dialog);
    auto *text=new QTextEdit(&dialog); text->setObjectName("text.content"); text->setAcceptRichText(false); text->setPlainText(content.toString());
    layout->addWidget(text);
    auto *error=new QLabel(&dialog); layout->addWidget(error);
    auto *buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog); layout->addWidget(buttons);
    connect(buttons,&QDialogButtonBox::accepted,&dialog,[&] {
        if(sendCommand({{"action","setTextContent"},{"name",text->toPlainText()}}) && sendCommand({{"action","toolApply"}})) dialog.accept();
        else error->setText(tr("Text could not be applied. Check the font and size."));
    });
    connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    if(dialog.exec()!=QDialog::Accepted) sendCommand({{"action","toolCancel"}});
    m_presentingText=false; updateOptionsBar(); refreshImage(); refreshLayers();
}

void SessionWindow::panCanvas(QPointF delta) {
    m_panOffset+=delta;
    if(m_canvasWidget) m_canvasWidget->update();
}
